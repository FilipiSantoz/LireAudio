use cpal::{
    I24,
    traits::{DeviceTrait, HostTrait},
};

use ::std::sync::atomic::{AtomicU64, Ordering};

use ringbuf::traits::{Consumer, Observer, Producer};

// --------------- Input Stream ------------
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
static INPUT_DROPPED_FRAMES: AtomicU64 = AtomicU64::new(0);
static OUTPUT_UNDERRUNS: AtomicU64 = AtomicU64::new(0);

pub fn dropped_input_frames() -> u64 {
    INPUT_DROPPED_FRAMES.load(Ordering::Relaxed)
}

pub fn output_underruns() -> u64 {
    OUTPUT_UNDERRUNS.load(Ordering::Relaxed)
}

pub fn select_input_device(host: &cpal::Host, filter_low: String) -> cpal::Device {
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
    let err_fn = |err| eprintln!("Input stream error: {err}");

    device.build_input_stream(
        config,
        move |data: &[T], _| {
            for frame in data.chunks(channels) {
                let left = frame[0].to_normalized_f32();
                let right = if channels > 1 {
                    frame[1].to_normalized_f32()
                } else {
                    left
                };

                let pair = [left, right];
                if producer.vacant_len() >= 2 {
                    let _ = producer.push_slice(&pair);
                } else {
                    INPUT_DROPPED_FRAMES.fetch_add(1, Ordering::Relaxed);
                }
            }
        },
        err_fn,
        None,
    )
}

pub fn build_input_stream_for_device(
    device: &cpal::Device,
    supported_config: cpal::SupportedStreamConfig,
    producer: ringbuf::HeapProd<f32>,
) -> Result<cpal::Stream, cpal::Error> {
    let mut config: cpal::StreamConfig = supported_config.into();

    config.buffer_size = cpal::BufferSize::Fixed(256);

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

// ---------------- Output Stream ---------------
pub trait FromNormalizedF32: Sized {
    fn from_normalized_f32(v: f32) -> Self;
}

impl FromNormalizedF32 for f32 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        v
    }
}

impl FromNormalizedF32 for f64 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        v.clamp(-1.0, 1.0) as f64
    }
}

impl FromNormalizedF32 for i8 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        (v.clamp(-1.0, 1.0) * i8::MAX as f32).round() as i8
    }
}

impl FromNormalizedF32 for i16 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        (v.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
    }
}

impl FromNormalizedF32 for I24 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        let val_f = (v.clamp(-1.0, 1.0) as f64 * 8_388_607.0f64).round();
        let val = (val_f as i64).clamp(-8_388_608, 8_388_607);
        I24::new(val as i32).expect("Fail to process I24 after clamping")
    }
}

impl FromNormalizedF32 for i32 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        (v.clamp(-1.0, 1.0) as f64 * i32::MAX as f64).round() as i32
    }
}

impl FromNormalizedF32 for i64 {
    #[inline(always)]
    fn from_normalized_f32(v: f32) -> Self {
        (v.clamp(-1.0, 1.0) as f64 * i64::MAX as f64).round() as i64
    }
}

pub fn select_output_device(host: &cpal::Host, filter_low: String) -> cpal::Device {
    let devices: Vec<_> = host
        .output_devices()
        .expect("Cannot list the output devices")
        .collect();

    println!("Available Output Devices:");
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
            println!("\nNone Output Device with '{filter_low}' name. Press Number of your Device:");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .expect("Fail to read line");
            input.trim().parse().expect("Type the right number")
        }
    };

    devices.into_iter().nth(index).expect("Index out of list")
}

pub fn create_output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut consumer: ringbuf::HeapCons<f32>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + FromNormalizedF32 + 'static,
{
    let channels = config.channels as usize;
    let err_fn = |err| eprintln!("Output stream error: {err}");
    // let underruns = Arc::new(AtomicU64::new(0));

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                let mut out_pair = [0.0f32; 2];
                let (left, right) = if consumer.occupied_len() >= 2 {
                    let _ = consumer.pop_slice(&mut out_pair);
                    (out_pair[0], out_pair[1])
                } else {
                    OUTPUT_UNDERRUNS.fetch_add(1, Ordering::Relaxed);
                    (0.0, 0.0)
                };

                if channels > 1 {
                    frame[0] = T::from_normalized_f32(left);
                    frame[1] = T::from_normalized_f32(right);
                } else {
                    frame[0] = T::from_normalized_f32((left + right) * 0.5);
                }
            }
        },
        err_fn,
        None,
    )
}

pub fn build_output_stream_for_device(
    device: &cpal::Device,
    supported_config: cpal::SupportedStreamConfig,
    consumer: ringbuf::HeapCons<f32>,
) -> Result<cpal::Stream, cpal::Error> {
    let mut config: cpal::StreamConfig = supported_config.into();
    config.buffer_size = cpal::BufferSize::Fixed(256);

    match supported_config.sample_format() {
        cpal::SampleFormat::F32 => create_output_stream::<f32>(device, config, consumer),
        cpal::SampleFormat::F64 => create_output_stream::<f64>(device, config, consumer),
        cpal::SampleFormat::I8 => create_output_stream::<i8>(device, config, consumer),
        cpal::SampleFormat::I16 => create_output_stream::<i16>(device, config, consumer),
        cpal::SampleFormat::I24 => create_output_stream::<I24>(device, config, consumer),
        cpal::SampleFormat::I32 => create_output_stream::<i32>(device, config, consumer),
        cpal::SampleFormat::I64 => create_output_stream::<i64>(device, config, consumer),
        other => panic!("Sample format is not supported: {other:?}"),
    }
}
