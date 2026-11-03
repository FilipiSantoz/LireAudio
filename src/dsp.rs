pub mod smoothing;

use rustfft::{FftPlanner, num_complex::Complex};
use smoothing::{BandSmoother, HIGH_FREQ_LIMIT};
use std::sync::Arc;

struct BandSpec {
    bin_start: usize,
    bin_end: usize,
    lo_bin: usize,
    hi_bin: usize,
    frac: f32,
}

pub struct SpectrumAnalyzer {
    fft_size: usize,

    fixed_min_db: f32,
    fixed_max_db: f32,

    window: Vec<f32>,
    window_sum: f32,

    fft: Arc<dyn rustfft::Fft<f32>>,
    band_specs: Vec<BandSpec>,
    smoother: BandSmoother,

    spectrum: Vec<Complex<f32>>,
    normalized_bins: Vec<f32>,
    display_bands: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize, num_bands: usize, sample_rate: u32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let window = hann_window(fft_size);
        let window_sum = window.iter().sum();

        let half = fft_size / 2;
        let (band_specs, center_freqs) = build_band_specs(num_bands, sample_rate, fft_size);
        let smoother = BandSmoother::new(&center_freqs, sample_rate);

        Self {
            fft_size,

            fixed_min_db: -58.0,
            fixed_max_db: -4.0,

            window,
            window_sum,
            fft,
            band_specs,
            smoother,

            spectrum: vec![Complex::new(0.0, 0.0); fft_size],
            normalized_bins: vec![0.0; half],
            display_bands: vec![0.0; num_bands],
        }
    }

    pub fn fixed_range(mut self, min_db: f32, max_db: f32) -> Self {
        self.fixed_min_db = min_db;
        self.fixed_max_db = max_db;
        self
    }

    pub fn analyze(&mut self, samples: &[f32]) -> &[f32] {
        debug_assert_eq!(samples.len(), self.fft_size);

        for (i, (&s, &w)) in samples.iter().zip(self.window.iter()).enumerate() {
            self.spectrum[i] = Complex::new(s * w, 0.0);
        }

        self.fft.process(&mut self.spectrum);

        let half = self.fft_size / 2;
        const EPS: f32 = 1e-12;
        let window_sum = self.window_sum.max(EPS);
        let max_db = self.fixed_max_db;
        let min_db = self.fixed_min_db;
        let range_db = (max_db - min_db).max(1e-3);

        for i in 0..half {
            let raw = self.spectrum[i].norm();

            let mag = if i == 0 {
                raw / window_sum
            } else {
                raw * (2.0f32 / window_sum)
            };

            let db = 20.0 * mag.max(EPS).log10();
            self.normalized_bins[i] = ((db - min_db) / range_db).clamp(0.0, 1.0);
        }

        for (i, (disp, spec)) in self
            .display_bands
            .iter_mut()
            .zip(&self.band_specs)
            .enumerate()
        {
            let current = if spec.bin_end > spec.bin_start + 1 {
                self.normalized_bins[spec.bin_start..spec.bin_end]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
            } else {
                self.normalized_bins[spec.lo_bin] * (1.0 - spec.frac)
                    + self.normalized_bins[spec.hi_bin] * spec.frac
            };

            self.smoother.update_band(i, disp, current);
        }

        &self.display_bands
    }
}

fn build_band_specs(
    num_bands: usize,
    sample_rate: u32,
    fft_size: usize,
) -> (Vec<BandSpec>, Vec<f32>) {
    let half_size = fft_size / 2;
    if num_bands == 0 || half_size == 0 || sample_rate == 0 {
        return (Vec::new(), Vec::new());
    }

    let min_freq = 30.0f32;
    let nyquist = sample_rate as f32 / 2.0;
    let max_freq = HIGH_FREQ_LIMIT.min(nyquist * 0.999);
    if max_freq <= min_freq {
        return (Vec::new(), Vec::new());
    }

    let ratio = max_freq / min_freq;
    let mut specs = Vec::with_capacity(num_bands);
    let mut center_freqs = Vec::with_capacity(num_bands);

    for i in 0..num_bands {
        let freq_start = min_freq * ratio.powf(i as f32 / num_bands as f32);
        let freq_end = min_freq * ratio.powf((i + 1) as f32 / num_bands as f32);

        let bin_start =
            ((freq_start * fft_size as f32 / sample_rate as f32) as usize).min(half_size - 1);
        let bin_end = ((freq_end * fft_size as f32 / sample_rate as f32) as usize)
            .max(bin_start + 1)
            .min(half_size);

        let center_freq = (freq_start * freq_end).sqrt();
        let center_bin = center_freq * fft_size as f32 / sample_rate as f32;

        let lo_bin = (center_bin.floor() as usize).min(half_size - 1);
        let hi_bin = (lo_bin + 1).min(half_size - 1);
        let frac = center_bin - lo_bin as f32;

        specs.push(BandSpec {
            bin_start,
            bin_end,
            lo_bin,
            hi_bin,
            frac,
        });
        center_freqs.push(center_freq);
    }

    (specs, center_freqs)
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
