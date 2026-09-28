use cpal::{
    I24,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use ringbuf::{
    HeapRb,
    traits::{Consumer, Producer, Split},
};

use rustfft::{FftPlanner, num_complex::Complex};

use std::{io::Write, thread};

const RING_SIZE: usize = 8192;
const FFT_SIZE: usize = 2048;

fn main() {
    let host = cpal::default_host();
    println!("Using Host : {:?}", host.id());

    let filter = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "monitor".to_string());
    let filter_low = filter.to_lowercase();

    let device = select_device(&host, filter_low);

    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown Device".to_string());

    println!("\n>>>Using Device: {}\n", device_name);

    let supported_config = device
        .default_input_config()
        .expect("Can't get the config from device");

    let sample_rate = supported_config.sample_rate();

    println!("Config: {:?}", supported_config);

    let rb = HeapRb::new(RING_SIZE);
    let (producer, consumer) = rb.split();

    let stream = build_stream_for_device(&device, supported_config, producer)
        .expect("Fail to create an Stream");

    let analysis_thread = thread::spawn(move || {
        analysis_loop(consumer, sample_rate);
    });

    stream.play().expect("fail to init stream");

    println!("Listening... Play music. Ctrl+C to quit");

    analysis_thread.join().ok();
}

fn hann_window(size: usize) -> Vec<f32> {
    if size <= 1 {
        return vec![1.0; size];
    }

    (0..size)
        .map(|i| {
            let x = i as f32 / (size as f32 - 1.0);
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * x).cos())
        })
        .collect()
}

fn select_device(host: &cpal::Host, filter_low: String) -> cpal::Device {
    let devices: Vec<_> = host
        .input_devices()
        .expect("Cannot list the input devices")
        .collect();

    println!("Available Input Devices");
    for (i, d) in devices.iter().enumerate() {
        if let Ok(desc) = d.description() {
            println!("  [{i}] {}", desc.name());
        }
    }

    let matched_index = devices.iter().position(|d| {
        d.description()
            .map(|desc| desc.name().to_lowercase().contains(&filter_low))
            .unwrap_or(false)
    });

    let index = match matched_index {
        Some(i) => i,
        None => {
            println!(
                "\nNone Device with '{filter_low}' name. Press Number \
                 of your Device (In the list):"
            );
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .expect("Fail to init Device");
            input.trim().parse().expect("Type the right number")
        }
    };

    devices.into_iter().nth(index).expect("Index out list")
}

trait ToNormalizedF32 {
    fn to_normalized_f32(&self) -> f32;
}

impl ToNormalizedF32 for f32 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        *self
    }
}

impl ToNormalizedF32 for f64 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        *self as f32
    }
}

impl ToNormalizedF32 for i8 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        *self as f32 / 128.0
    }
}

impl ToNormalizedF32 for i16 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        *self as f32 / 32768.0
    }
}
impl ToNormalizedF32 for I24 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        self.inner() as f32 / 8388607.0
    }
}

impl ToNormalizedF32 for i32 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        *self as f32 / 2147483648.0
    }
}

impl ToNormalizedF32 for i64 {
    #[inline(always)]
    fn to_normalized_f32(&self) -> f32 {
        *self as f32 / 9223372036854775808.0
    }
}

fn create_input_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: ringbuf::HeapProd<f32>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + ToNormalizedF32 + 'static,
{
    let channels = config.channels as usize;

    device.build_input_stream(
        config,
        move |data: &[T], _| {
            for frame in data.chunks(channels) {
                let mono: f32 =
                    frame.iter().map(|s| s.to_normalized_f32()).sum::<f32>() / channels as f32;

                let _ = producer.try_push(mono);
            }
        },
        |err| eprintln!("Stream error: {err}"),
        None,
    )
}

fn build_stream_for_device(
    device: &cpal::Device,
    supported_config: cpal::SupportedStreamConfig,
    producer: ringbuf::HeapProd<f32>,
) -> Result<cpal::Stream, cpal::Error> {
    let config: cpal::StreamConfig = supported_config.clone().into();

    match supported_config.sample_format() {
        cpal::SampleFormat::F32 => create_input_stream::<f32>(device, config, producer),
        cpal::SampleFormat::F64 => create_input_stream::<f64>(device, config, producer),
        cpal::SampleFormat::I8 => create_input_stream::<i8>(device, config, producer),
        cpal::SampleFormat::I16 => create_input_stream::<i16>(device, config, producer),
        cpal::SampleFormat::I24 => create_input_stream::<I24>(device, config, producer),
        cpal::SampleFormat::I32 => create_input_stream::<i32>(device, config, producer),
        cpal::SampleFormat::I64 => create_input_stream::<i64>(device, config, producer),
        other => panic!("Sample format not supported: {other:?}"),
    }
}

fn analysis_loop(mut consumer: ringbuf::HeapCons<f32>, sample_rate: u32) {
    let mut buffer: Vec<f32> = Vec::with_capacity(FFT_SIZE);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let window = hann_window(FFT_SIZE);

    loop {
        match consumer.try_pop() {
            Some(sample) => {
                buffer.push(sample);

                if buffer.len() >= FFT_SIZE {
                    process_fft_block(&buffer, &window, &*fft, sample_rate);
                    buffer.clear();
                }
            }
            None => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}

fn process_fft_block(
    samples: &[f32],
    window: &[f32],
    fft: &dyn rustfft::Fft<f32>,
    sample_rate: u32,
) {
    let mut spectrum: Vec<Complex<f32>> = samples
        .iter()
        .zip(window)
        .map(|(s, w)| Complex::new(s * w, 0.0))
        .collect();

    fft.process(&mut spectrum);

    let (peak_bin, peak_mag) = spectrum[..FFT_SIZE / 2]
        .iter()
        .enumerate()
        .map(|(i, c)| (i, c.norm()))
        .fold((0usize, 0.0f32), |acc, x| if x.1 > acc.1 { x } else { acc });

    let freq = peak_bin as f32 * sample_rate as f32 / FFT_SIZE as f32;
    print!("\rPico: {freq:>8.1} Hz   mag: {peak_mag:>8.3}   ");
    std::io::stdout().flush().ok();
}
