use crate::audio_capture::ProcessEnds;
use crate::dsp::SpectrumAnalyzer;
use crate::fx::StereoFxChain;
use ringbuf::traits::{Consumer, Producer};
use std::collections::VecDeque;
// use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const FFT_SIZE: usize = 1024;
pub const NUM_BANDS: usize = 34;
pub const HOP_SIZE: usize = 256;
const JITTER_MARGIN: Duration = Duration::from_millis(8);
const MIN_DELAY: Duration = Duration::from_millis(10);
const MAX_DELAY: Duration = Duration::from_millis(100);
const STEREO: usize = 2;
const FRAME_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy)]
pub struct TimedFrame {
    pub present_at: Instant,
    pub bands: [f32; NUM_BANDS],
}

pub type FrameQueue = Arc<Mutex<VecDeque<TimedFrame>>>;

pub fn new_frame_queue() -> FrameQueue {
    Arc::new(Mutex::new(VecDeque::with_capacity(FRAME_QUEUE_CAPACITY)))
}

pub struct AudioProcessor {
    pub process: ProcessEnds,
    pub sample_rate: u32,
    pub stereo_fx: StereoFxChain,
    pub stop: Arc<AtomicBool>,
    pub frames: FrameQueue,
    pub chunk_hint: Arc<AtomicUsize>,
}

impl AudioProcessor {
    pub fn run(self) {
        let AudioProcessor {
            process,
            sample_rate,
            mut stereo_fx,
            stop,
            frames,
            chunk_hint,
        } = self;

        if sample_rate == 0 {
            return;
        }
        let ProcessEnds {
            mut input,
            mut output,
        } = process;

        stereo_fx.prepare(sample_rate);

        let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 8);
        let mut temp = vec![0.0f32; 4096];

        let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate)
            .attack(0.78)
            .fixed_range(-60.0, 0.0)
            .release(0.26);

        let hop_duration = Duration::from_secs_f64(HOP_SIZE as f64 / sample_rate as f64);
        let max_step = hop_duration.mul_f64(0.75);

        let mut samples_seen: u64 = 0;

        let mut origin: Option<Instant> = None;
        let mut current_delay: Option<Duration> = None;
        let mut last_present: Option<Instant> = None;
        let mut idle_since: Option<Instant> = None;

        while !stop.load(Ordering::Relaxed) {
            let count = input.pop_slice(&mut temp);
            if count == 0 {
                let since = *idle_since.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_millis(250) && origin.is_some() {
                    buffer.clear();
                    samples_seen = 0;
                    origin = None;
                    current_delay = None;
                    idle_since = None;
                    last_present = None;
                    if let Ok(mut q) = frames.lock() {
                        q.clear();
                    }
                }
                thread::sleep(Duration::from_millis(1));
                continue;
            } else {
                idle_since = None;
            }

            let count = count - (count % STEREO);
            let chunk = &mut temp[..count];

            if !stereo_fx.is_empty() {
                stereo_fx.process_block_interleaved(chunk);
            }

            let _ = output.push_slice(chunk);

            for &[l, r] in chunk.as_chunks::<STEREO>().0 {
                buffer.push((l + r) * 0.5);
            }
            let quantum = chunk_hint.load(Ordering::Relaxed);
            let batch_frames = count / STEREO;
            let chunk_frames = if quantum > 0 { quantum } else { batch_frames };
            samples_seen += batch_frames as u64;

            let t0 = *origin.get_or_insert_with(|| {
                let now = Instant::now();

                let elapsed = Duration::from_secs_f64(samples_seen as f64 / sample_rate as f64);
                now.checked_sub(elapsed).unwrap_or(now)
            });

            let target_delay = required_delay(chunk_frames, HOP_SIZE, sample_rate);

            while buffer.len() >= FFT_SIZE {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let window_end = samples_seen.saturating_sub((buffer.len() - FFT_SIZE) as u64);
                let window_time = Duration::from_secs_f64(window_end as f64 / sample_rate as f64);

                let mut bands = [0.0f32; NUM_BANDS];
                bands.copy_from_slice(analyzer.analyze(&buffer[..FFT_SIZE]));

                let delay = next_delay(current_delay, target_delay, max_step);

                current_delay = Some(delay);
                let candidate = t0 + window_time + delay;
                let present_at = last_present.map_or(candidate, |last| candidate.max(last));
                last_present = Some(present_at);

                push_frame(&frames, TimedFrame { present_at, bands });

                let new_len = buffer.len() - HOP_SIZE;
                buffer.copy_within(HOP_SIZE.., 0);
                buffer.truncate(new_len);
            }
        }
    }
}

fn required_delay(chunk_frames: usize, hop: usize, sample_rate: u32) -> Duration {
    let span =
        Duration::from_secs_f64(chunk_frames.saturating_sub(hop) as f64 / sample_rate as f64);
    (span + JITTER_MARGIN).clamp(MIN_DELAY, MAX_DELAY)
}

fn next_delay(current: Option<Duration>, target: Duration, max_step: Duration) -> Duration {
    match current {
        None => target,
        Some(c) if target >= c => (c + max_step).min(target),
        Some(c) => c.saturating_sub(max_step).max(target),
    }
}

fn push_frame(queue: &FrameQueue, frame: TimedFrame) {
    if let Ok(mut q) = queue.lock() {
        if q.len() >= FRAME_QUEUE_CAPACITY {
            q.pop_front();
        }
        q.push_back(frame);
    }
}

pub fn take_due_frame(queue: &FrameQueue, now: Instant) -> Option<[f32; NUM_BANDS]> {
    let mut q = queue.lock().ok()?;
    let mut latest = None;

    while let Some(front) = q.front() {
        if front.present_at > now {
            break;
        }
        latest = q.pop_front().map(|f| f.bands);
    }

    latest
}

pub fn next_present_at(queue: &FrameQueue) -> Option<Instant> {
    queue.lock().ok()?.front().map(|f| f.present_at)
}
// ------ Terminal Render ------
// pub fn run_analysis(mut consumer: ringbuf::HeapCons<f32>, sample_rate: u32) {
//     let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 4);
//     let mut temp_buffer = vec![0.0f32; 1024];
//
//     let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate)
//         .attack(0.8)
//         .release(0.15);
//
//     loop {
//         let count = consumer.pop_slice(&mut temp_buffer);
//
//         if count > 0 {
//             buffer.extend_from_slice(&temp_buffer[..count]);
//
//             while buffer.len() >= FFT_SIZE {
//                 let bands = analyzer.analyze(&buffer[..FFT_SIZE]);
//                 print_equalizer(bands);
//
//                 let new_len = buffer.len() - HOP_SIZE;
//                 buffer.copy_within(HOP_SIZE.., 0);
//                 buffer.truncate(new_len);
//             }
//         } else {
//             thread::sleep(Duration::from_millis(1));
//         }
//     }
// }
//
// fn print_equalizer(bands: &[f32]) {
//     let blocks = [' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
//
//     let mut output = String::new();
//     output.push_str("\rEQ: [");
//
//     for &val in bands {
//         let idx = (val * (blocks.len() - 1) as f32).round() as usize;
//         let clamped_idx = idx.min(blocks.len() - 1);
//         output.push(blocks[clamped_idx]);
//     }
//
//     output.push(']');
//
//     print!("{output}");
//     std::io::stdout().flush().ok();
// }
