use crate::dsp::SpectrumAnalyzer;
use crate::fx::StereoFxChain;
use crate::metrics::AudioMetrics;
use ringbuf::traits::{Consumer, Observer, Producer};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const FFT_SIZE: usize = 1024;
pub const NUM_BANDS: usize = 26;
const HOP_SIZE: usize = 256;

pub fn run_audio_processor<F>(
    mut consumer: ringbuf::HeapCons<f32>,
    mut output: Option<ringbuf::HeapProd<f32>>,
    sample_rate: u32,
    mut stereo_fx: StereoFxChain,
    stop: Arc<AtomicBool>,
    mut on_bands: F,
    metrics: Arc<AudioMetrics>,
) where
    F: FnMut(&[f32]),
{
    stereo_fx.prepare(sample_rate);

    let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 8);
    let mut temp_buffer = vec![0.0f32; 8192];

    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate)
        .attack(0.60)
        .release(0.15)
        .fixed_range(-80.0, 0.0);

    let mut latest_bands = vec![0.0f32; NUM_BANDS];
    let mut has_bands = false;

    let mut last_emit = std::time::Instant::now();
    let emit_interval = Duration::from_millis(8);

    let mut report_timer = std::time::Instant::now();
    let mut queue_peak_samples = 0usize;
    let mut processed_frames_period = 0u64;
    let mut fft_windows_period = 0u64;
    let mut max_batch_us = 0u64;
    let mut fresh_emits_period = 0u64;
    let mut stale_emits_period = 0u64;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        queue_peak_samples = queue_peak_samples.max(consumer.occupied_len());

        let count = consumer.pop_slice(&mut temp_buffer);

        if count > 0 {
            let count = count - (count % 2);

            if count == 0 {
                continue;
            }

            let batch_start = std::time::Instant::now();
            let chunk = &mut temp_buffer[..count];

            if !stereo_fx.is_empty() {
                stereo_fx.process_block_interleaved(chunk);
            }

            if let Some(out) = output.as_mut() {
                let _ = out.push_slice(chunk);
            }

            for &[l, r] in chunk.as_chunks::<2>().0 {
                let mono_for_fft = (l + r) * 0.5;
                buffer.push(mono_for_fft);
            }

            while buffer.len() >= FFT_SIZE {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let bands = analyzer.analyze(&buffer[..FFT_SIZE]);

                latest_bands.copy_from_slice(bands);
                has_bands = true;
                fft_windows_period += 1;

                let new_len = buffer.len() - HOP_SIZE;
                buffer.copy_within(HOP_SIZE.., 0);
                buffer.truncate(new_len);
            }

            processed_frames_period += (count / 2) as u64;

            max_batch_us = max_batch_us.max(batch_start.elapsed().as_micros() as u64);
        } else {
            thread::sleep(Duration::from_millis(1));
        }

        if last_emit.elapsed() >= emit_interval {
            on_bands(&latest_bands);
            if has_bands {
                fresh_emits_period += 1;
                has_bands = false;
            } else {
                stale_emits_period += 1;
            }
            last_emit = std::time::Instant::now();
        }

        let elapsed = report_timer.elapsed();

        if elapsed >= Duration::from_secs(1) {
            let seconds = elapsed.as_secs_f64();

            // Contadores acumulados pelo callback PipeWire.
            let callbacks = metrics.capture_callbacks.swap(0, Ordering::Relaxed);

            let input_frames = metrics.capture_frames.swap(0, Ordering::Relaxed);

            let enqueued_frames = metrics.enqueued_frames.swap(0, Ordering::Relaxed);

            let dropped_frames = metrics.dropped_frames.swap(0, Ordering::Relaxed);

            let max_callback_frames = metrics.max_callback_frames.swap(0, Ordering::Relaxed);

            let last_callback_frames = metrics.last_callback_frames.load(Ordering::Relaxed);

            let avg_callback_frames = if callbacks > 0 {
                input_frames as f64 / callbacks as f64
            } else {
                0.0
            };

            let total_attempted = enqueued_frames + dropped_frames;

            let drop_percent = if total_attempted > 0 {
                dropped_frames as f64 * 100.0 / total_attempted as f64
            } else {
                0.0
            };

            // occupied_len() conta elementos f32, não frames estéreo.
            let queue_peak_ms = queue_peak_samples as f64 / (sample_rate as f64 * 2.0) * 1000.0;
            let total_emits = fresh_emits_period + stale_emits_period;
            eprintln!(
                concat!(
                    "[Luna metrics] ",
                    "callbacks={:.1}/s, ",
                    "input={:.0} frames/s, ",
                    "callback avg/max/last={:.0}/{}/{}, ",
                    "enqueued={:.0} frames/s, ",
                    "dropped={:.0} frames/s ({:.2}%), ",
                    "queue_peak={} samples ({:.1} ms), ",
                    "processed={:.0} frames/s, ",
                    "FFT={:.1}/s, ",
                    "GUI_total_emit={:.1}/s (fresh_emit={:.1}/s, repeated={:.1}/s), ",
                    "max_batch={:.2} ms"
                ),
                callbacks as f64 / seconds,
                input_frames as f64 / seconds,
                avg_callback_frames,
                max_callback_frames,
                last_callback_frames,
                enqueued_frames as f64 / seconds,
                dropped_frames as f64 / seconds,
                drop_percent,
                queue_peak_samples,
                queue_peak_ms,
                processed_frames_period as f64 / seconds,
                fft_windows_period as f64 / seconds,
                total_emits as f64 / seconds,
                fresh_emits_period as f64 / seconds,
                stale_emits_period as f64 / seconds,
                max_batch_us as f64 / 1000.0,
            );

            // Começa um novo período de medição.
            queue_peak_samples = 0;
            processed_frames_period = 0;
            fft_windows_period = 0;
            fresh_emits_period = 0;
            stale_emits_period = 0;
            max_batch_us = 0;
            report_timer = std::time::Instant::now();
        }
    }
}
// ------ Terminal Render ------
pub fn run_analysis(mut consumer: ringbuf::HeapCons<f32>, sample_rate: u32) {
    let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 4);
    let mut temp_buffer = vec![0.0f32; 1024];

    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate)
        .attack(0.8)
        .release(0.15);

    loop {
        let count = consumer.pop_slice(&mut temp_buffer);

        if count > 0 {
            buffer.extend_from_slice(&temp_buffer[..count]);

            while buffer.len() >= FFT_SIZE {
                let bands = analyzer.analyze(&buffer[..FFT_SIZE]);
                print_equalizer(bands);

                let new_len = buffer.len() - HOP_SIZE;
                buffer.copy_within(HOP_SIZE.., 0);
                buffer.truncate(new_len);
            }
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

fn print_equalizer(bands: &[f32]) {
    let blocks = [' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let mut output = String::new();
    output.push_str("\rEQ: [");

    for &val in bands {
        let idx = (val * (blocks.len() - 1) as f32).round() as usize;
        let clamped_idx = idx.min(blocks.len() - 1);
        output.push(blocks[clamped_idx]);
    }

    output.push(']');

    print!("{output}");
    std::io::stdout().flush().ok();
}
