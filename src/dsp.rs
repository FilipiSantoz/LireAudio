use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;

pub struct SpectrumAnalyzer {
    pub fft_size: usize,
    pub sample_rate: u32,
    pub num_bands: usize,
    pub smoothing: f32,
    pub decay_rate: f32,

    window: Vec<f32>,
    window_sum: f32,
    fft: Arc<dyn rustfft::Fft<f32>>,

    spectrum: Vec<Complex<f32>>,
    smoothed_magnitudes: Vec<f32>,
    display_bands: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize, num_bands: usize, sample_rate: u32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let window = hann_window(fft_size);
        let window_sum = window.iter().sum();

        Self {
            fft_size,
            sample_rate,
            num_bands,
            smoothing: 0.7,
            decay_rate: 0.92,
            window,
            window_sum,
            fft,
            spectrum: vec![Complex::new(0.0, 0.0); fft_size],
            smoothed_magnitudes: vec![0.0; fft_size / 2],
            display_bands: vec![0.0; num_bands],
        }
    }

    pub fn analyze(&mut self, samples: &[f32]) -> &[f32] {
        assert_eq!(samples.len(), self.fft_size);

        for (i, (&s, &w)) in samples.iter().zip(&self.window).enumerate() {
            self.spectrum[i] = Complex::new(s * w, 0.0);
        }

        self.fft.process(&mut self.spectrum);

        let half = self.fft_size / 2;
        let min_db = -80.0f32;
        let max_db = 0.0f32;

        for i in 0..half {
            let mag = self.spectrum[i].norm() / self.window_sum;
            let db = 20.0 * mag.max(1e-9).log10();
            let norm = ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);

            self.smoothed_magnitudes[i] =
                self.smoothed_magnitudes[i] * self.smoothing + norm * (1.0 - self.smoothing);
        }

        let bands = group_into_log_bands(
            &self.smoothed_magnitudes,
            self.sample_rate,
            self.fft_size,
            self.num_bands,
        );

        for (disp, current) in self.display_bands.iter_mut().zip(bands) {
            if current > *disp {
                *disp = current;
            } else {
                *disp *= self.decay_rate;
            }
        }

        &self.display_bands
    }
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

fn group_into_log_bands(
    magnitudes: &[f32],
    sample_rate: u32,
    fft_size: usize,
    num_bands: usize,
) -> Vec<f32> {
    let min_freq = 30.0f32;
    let max_freq = 16000.0f32;
    let half_size = magnitudes.len();

    let mut bands = vec![0.0f32; num_bands];

    for (i, band) in bands.iter_mut().enumerate() {
        let freq_start = min_freq * (max_freq / min_freq).powf(i as f32 / num_bands as f32);
        let freq_end = min_freq * (max_freq / min_freq).powf((i + 1) as f32 / num_bands as f32);

        let bin_start =
            ((freq_start * fft_size as f32 / sample_rate as f32) as usize).min(half_size - 1);
        let bin_end = ((freq_end * fft_size as f32 / sample_rate as f32) as usize)
            .max(bin_start + 1)
            .min(half_size);

        let mut max_val = 0.0f32;

        for &val in &magnitudes[bin_start..bin_end] {
            max_val = max_val.max(val);
        }

        *band = max_val;
    }

    bands
}
