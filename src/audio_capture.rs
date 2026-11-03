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

pub struct AnalysisTap(pub HeapProd<f32>);

pub struct AnalysisInput(pub HeapCons<f32>);

pub fn create_analysis_pipe(capacity: usize) -> (AnalysisTap, AnalysisInput) {
    let (prod, cons) = HeapRb::<f32>::new(capacity).split();
    (AnalysisTap(prod), AnalysisInput(cons))
}
