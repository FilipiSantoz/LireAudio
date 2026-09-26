use cpal::{
    I24,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    io::Write,
    time::{Duration, Instant},
};

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

    println!("Config: {:?}", supported_config);

    let stream =
        build_stream_for_device(&device, supported_config).expect("Fail to create an Stream");

    stream.play().expect("fail to init stream");

    println!("Capturing... Play music. Ctrl+C to quit");
    loop {
        std::thread::sleep(Duration::from_millis(500));
    }
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
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + ToNormalizedF32 + 'static,
{
    let print_interval = Duration::from_millis(50);
    let mut last_print = Instant::now();
    let err_fn = |err| eprintln!("\nErro na stream: {err}");

    device.build_input_stream(
        config,
        move |data: &[T], _| {
            if last_print.elapsed() >= print_interval {
                let peak = data
                    .iter()
                    .fold(0.0f32, |max, s| max.max(s.to_normalized_f32().abs()));

                print_bar(peak);
                last_print = Instant::now();
            }
        },
        err_fn,
        None,
    )
}

fn build_stream_for_device(
    device: &cpal::Device,
    supported_config: cpal::SupportedStreamConfig,
) -> Result<cpal::Stream, cpal::Error> {
    let config: cpal::StreamConfig = supported_config.clone().into();

    match supported_config.sample_format() {
        cpal::SampleFormat::F32 => create_input_stream::<f32>(device, config),
        cpal::SampleFormat::F64 => create_input_stream::<f64>(device, config),
        cpal::SampleFormat::I8 => create_input_stream::<i8>(device, config),
        cpal::SampleFormat::I16 => create_input_stream::<i16>(device, config),
        cpal::SampleFormat::I24 => create_input_stream::<I24>(device, config),
        cpal::SampleFormat::I32 => create_input_stream::<i32>(device, config),
        cpal::SampleFormat::I64 => create_input_stream::<i64>(device, config),
        other => panic!("Sample format not supported: {other:?}"),
    }
}

fn print_bar(peak: f32) {
    let width = 50usize;
    let filled = ((peak * width as f32) as usize).min(width);
    let bar: String = "#".repeat(filled) + &" ".repeat(width - filled);
    print!("\r[{bar}] {peak:.3}");
    std::io::stdout().flush().ok();
}
