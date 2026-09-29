use crate::dsp::SpectrumAnalyzer;
use ringbuf::traits::Consumer;
use std::io::Write;
use std::thread;
use std::time::Duration;

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 16;
const HOP_SIZE: usize = 512;

pub fn run_analysis(mut consumer: ringbuf::HeapCons<f32>, sample_rate: u32) {
    let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE * 2);
    let mut temp_buffer = vec![0.0f32; 256];

    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE, NUM_BANDS, sample_rate);

    loop {
        let count = consumer.pop_slice(&mut temp_buffer);

        if count > 0 {
            buffer.extend_from_slice(&temp_buffer[..count]);

            while buffer.len() >= FFT_SIZE {
                let bands = analyzer.analyze(&buffer[..FFT_SIZE]);
                print_equalizer(bands);

                buffer.drain(0..HOP_SIZE);
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
