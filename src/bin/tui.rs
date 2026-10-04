use cpal::traits::{DeviceTrait, StreamTrait};
use ringbuf::{HeapRb, traits::Split};
use std::thread;

use luna_analyzer::audio::{build_input_stream_for_device, select_input_device};
use luna_analyzer::pipeline::run_analysis;

const RING_SIZE: usize = 16384;

fn main() {
    let host = cpal::default_host();
    println!("Using Host: {:?}", host.id());

    let filter = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Monitor".to_string());
    let filter_low = filter.to_lowercase();

    let device = select_input_device(&host, filter_low);

    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown Device".to_string());

    println!("\n>>> Using Device: {}\n", device_name);

    let supported_config = device
        .default_input_config()
        .expect("Can't get config from device");

    let sample_rate = supported_config.sample_rate();

    let rb = HeapRb::new(RING_SIZE);
    let (producer, consumer) = rb.split();

    let stream = build_input_stream_for_device(&device, supported_config, producer)
        .expect("Fail to create stream");

    let analysis_thread = thread::spawn(move || {
        run_analysis(consumer, sample_rate);
    });

    stream.play().expect("Fail to init stream");

    println!("Listening... Play music. Ctrl+C to quit");
    analysis_thread.join().ok();
}
