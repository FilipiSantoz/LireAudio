use crate::pipeline::HOP_SIZE;
use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;

// Smoothing
const ATTACK_TAU: f32 = 0.018;
const RELEASE_TAU: f32 = 0.060;
const STRONG_RISE: f32 = 0.10;
const NOISE_FLOOR: f32 = 0.02;

const BASS_FREQ_LIMIT: f32 = 250.0;
const MID_FREQ_LIMIT: f32 = 3500.0;
const HIGH_FREQ_LIMIT: f32 = 16000.0;

const BASS_ENERGY_WEIGHT: f32 = 0.90;
const MID_ENERGY_WEIGHT: f32 = 0.72;
const HIGH_ENERGY_WEIGHT: f32 = 0.68;

// Gamma per band
// Higher Value = -Energy
const BASS_GAMMA: f32 = 1.05;
const MID_GAMMA: f32 = 1.30;
const HIGH_GAMMA: f32 = 1.40;

struct BandSpec {
    bin_start: usize,
    bin_end: usize,
    lo_bin: usize,
    hi_bin: usize,
    frac: f32,
    weight: f32,
    gamma: f32,
}

pub struct SpectrumAnalyzer {
    fft_size: usize,

    attack_k: f32,
    release_k: f32,

    fixed_min_db: f32,
    fixed_max_db: f32,

    window: Vec<f32>,
    window_sum: f32,

    fft: Arc<dyn rustfft::Fft<f32>>,
    band_specs: Vec<BandSpec>,

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
        let band_specs = build_band_specs(num_bands, sample_rate, fft_size);

        let hop_dt = if sample_rate > 0 {
            HOP_SIZE as f32 / sample_rate as f32
        } else {
            0.0053
        };
        let attack_k = 1.0 - (-hop_dt / ATTACK_TAU).exp();
        let release_k = 1.0 - (-hop_dt / RELEASE_TAU).exp();

        Self {
            fft_size,
            attack_k,
            release_k,

            fixed_min_db: -58.0,
            fixed_max_db: -4.0,

            window,
            window_sum,
            fft,
            band_specs,

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

        if samples.len() != self.fft_size {
            return &self.display_bands;
        }

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

        for (disp, spec) in self.display_bands.iter_mut().zip(&self.band_specs) {
            let current = if spec.bin_end > spec.bin_start + 1 {
                self.normalized_bins[spec.bin_start..spec.bin_end]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
            } else {
                self.normalized_bins[spec.lo_bin] * (1.0 - spec.frac)
                    + self.normalized_bins[spec.hi_bin] * spec.frac
            };

            let weighted = (current * spec.weight).clamp(0.0, 1.0);

            let gated = if weighted <= NOISE_FLOOR {
                0.0
            } else {
                (weighted - NOISE_FLOOR) / (1.0 - NOISE_FLOOR)
            };

            let target = gated.powf(spec.gamma);
            let delta = target - *disp;

            if delta >= STRONG_RISE {
                *disp = target;
            } else if delta > 0.0 {
                *disp += delta * self.attack_k;
            } else {
                *disp += delta * self.release_k;
            }
        }

        &self.display_bands
    }
}

fn build_band_specs(num_bands: usize, sample_rate: u32, fft_size: usize) -> Vec<BandSpec> {
    let half_size = fft_size / 2;
    if num_bands == 0 || half_size == 0 || sample_rate == 0 {
        return Vec::new();
    }

    let min_freq = 30.0f32;
    let nyquist = sample_rate as f32 / 2.0;
    let max_freq = HIGH_FREQ_LIMIT.min(nyquist * 0.999);
    if max_freq <= min_freq {
        return Vec::new();
    }

    let ratio = max_freq / min_freq;
    let mut specs = Vec::with_capacity(num_bands);

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

        let (weight, gamma) = band_energy_profile(center_freq);

        specs.push(BandSpec {
            bin_start,
            bin_end,
            lo_bin,
            hi_bin,
            frac,
            weight,
            gamma,
        });
    }

    specs
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn band_energy_profile(freq: f32) -> (f32, f32) {
    let log_f = freq.max(20.0).ln();
    let log_bass = BASS_FREQ_LIMIT.ln();
    let log_mid = MID_FREQ_LIMIT.ln();
    let log_high = HIGH_FREQ_LIMIT.ln();

    if log_f <= log_bass {
        (BASS_ENERGY_WEIGHT, BASS_GAMMA)
    } else if log_f <= log_mid {
        let t = smoothstep(log_bass, log_mid, log_f);
        (
            lerp(BASS_ENERGY_WEIGHT, MID_ENERGY_WEIGHT, t),
            lerp(BASS_GAMMA, MID_GAMMA, t),
        )
    } else {
        let t = smoothstep(log_mid, log_high, log_f);
        (
            lerp(MID_ENERGY_WEIGHT, HIGH_ENERGY_WEIGHT, t),
            lerp(MID_GAMMA, HIGH_GAMMA, t),
        )
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
