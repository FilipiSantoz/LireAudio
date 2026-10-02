use cpal::{
    I24,
    traits::{DeviceTrait, HostTrait},
};

use ringbuf::traits::Producer;

pub trait ToNormalizedF32 {
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

pub fn select_device(host: &cpal::Host, filter_low: String) -> cpal::Device {
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

    let matches: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let name = d.description().ok()?.name().to_lowercase();
            if name.contains(&filter_low) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    let index = if matches.len() == 1 {
        matches[0]
    } else {
        println!("\nSelect a device index from the list above:");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        input
            .trim()
            .parse::<usize>()
            .expect("Please type a valid index number")
    };

    devices.into_iter().nth(index).expect("Index out of range")
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

pub fn build_stream_for_device(
    device: &cpal::Device,
    supported_config: cpal::SupportedStreamConfig,
    producer: ringbuf::HeapProd<f32>,
) -> Result<cpal::Stream, cpal::Error> {
    let mut config: cpal::StreamConfig = supported_config.into();

    config.buffer_size = cpal::BufferSize::Fixed(512);

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
