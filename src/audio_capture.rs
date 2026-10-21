#[cfg(target_os = "linux")]
pub mod pipewire;

// #[cfg(target_os = "windows")]
// pub mod wasapi;

#[cfg(target_os = "linux")]
pub use pipewire::PipewireEngine;

use ringbuf::{HeapCons, HeapProd, HeapRb, traits::Split};

pub const INPUT_CAPACITY: usize = 4_096;
pub const OUTPUT_CAPACITY: usize = 8_192;
pub const PIPEWIRE_SAMPLE_RATE: u32 = 48_000;

pub struct CaptureEnd(pub HeapProd<f32>);

pub struct ProcessEnds {
    pub input: HeapCons<f32>,
    pub output: HeapProd<f32>,
}

pub struct PlaybackEnd(pub HeapCons<f32>);

pub fn create_pipes(
    input_capacity: usize,
    output_capacity: usize,
) -> (CaptureEnd, ProcessEnds, PlaybackEnd) {
    let (in_prod, in_cons) = HeapRb::<f32>::new(input_capacity).split();
    let (out_prod, out_cons) = HeapRb::<f32>::new(output_capacity).split();
    (
        CaptureEnd(in_prod),
        ProcessEnds {
            input: in_cons,
            output: out_prod,
        },
        PlaybackEnd(out_cons),
    )
}
