use crate::dsp::SpectrumAnalyzer;
use ringbuf::traits::Consumer;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 26;
const HOP_SIZE: usize = 512;

pub fn run_analysis_stoppable<F>(
    mut input: ringbuf::HeapCons<f32>,
    mut output: Option<ringbuf::HeapProd<f32>>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
    mut on_bands: F,
) where
    F: FnMut(&[f32]),
{
    let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 4);
    let mut temp_buffer = vec![0.0f32; 1024];
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate)
        .auto_gain(false)
        .smoothing(0.80)
        .fixed_range(-70.0, 0.0)
        .decay_rate(0.34);

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let count = input.pop_slice(&mut temp_buffer);

        if count > 0 {
            buffer.extend_from_slice(&temp_buffer[..count]);

            while buffer.len() >= FFT_SIZE {
                let bands = analyzer.analyze(&buffer[..FFT_SIZE]);
                on_bands(bands);

                let new_len = buffer.len() - HOP_SIZE;
                buffer.copy_within(HOP_SIZE.., 0);
                buffer.truncate(new_len);
            }
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

// ------ Terminal Render ------
pub fn run_analysis(mut consumer: ringbuf::HeapCons<f32>, sample_rate: u32) {
    let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 4);
    let mut temp_buffer = vec![0.0f32; 1024];

    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate)
        .auto_gain(true)
        .smoothing(0.8)
        .decay_rate(0.15);

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
