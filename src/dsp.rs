use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;

pub struct SpectrumAnalyzer {
    fft_size: usize,
    sample_rate: u32,
    num_bands: usize,

    smoothing: f32,
    decay_rate: f32,

    auto_gain: bool,
    fixed_min_db: f32,
    fixed_max_db: f32,

    floor_db: f32,

    window: Vec<f32>,
    window_sum: f32,

    fft: Arc<dyn rustfft::Fft<f32>>,

    spectrum: Vec<Complex<f32>>,
    smoothed_db: Vec<f32>,
    normalized_bins: Vec<f32>,
    db_scratch: Vec<f32>,
    display_bands: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize, num_bands: usize, sample_rate: u32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let window = hann_window(fft_size);
        let window_sum = window.iter().sum();

        let half = fft_size / 2;

        Self {
            fft_size,
            sample_rate,
            num_bands,
            smoothing: 0.7,
            decay_rate: 0.15,

            auto_gain: false,
            fixed_min_db: -80.0,
            fixed_max_db: 0.0,

            floor_db: -80.0,

            window,
            window_sum,
            fft,

            spectrum: vec![Complex::new(0.0, 0.0); fft_size],
            smoothed_db: vec![-80.0; half],
            normalized_bins: vec![0.0; half],
            db_scratch: Vec::with_capacity(half),
            display_bands: vec![0.0; num_bands],
        }
    }

    pub fn auto_gain(mut self, enabled: bool) -> Self {
        self.auto_gain = enabled;
        self
    }

    pub fn smoothing(mut self, smoothing: f32) -> Self {
        self.smoothing = smoothing.clamp(0.0, 0.95);
        self
    }

    pub fn decay_rate(mut self, decay_rate: f32) -> Self {
        self.decay_rate = decay_rate.clamp(0.0, 0.99);
        self
    }

    pub fn fixed_range(mut self, min_db: f32, max_db: f32) -> Self {
        self.fixed_min_db = min_db;
        self.fixed_max_db = max_db;
        self.floor_db = min_db;
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

        for i in 0..half {
            let raw = self.spectrum[i].norm();

            const EPS: f32 = 1e-12;

            let mag = if i == 0 || (i == half - 1 && self.fft_size.is_multiple_of(2)) {
                raw / self.window_sum.max(EPS)
            } else {
                raw * (2.0f32 / self.window_sum.max(EPS))
            };

            let db = 20.0 * mag.max(EPS).log10();

            let attack = self.smoothing.clamp(0.001, 0.99);
            let release = self.decay_rate.clamp(0.001, 0.99);
            let alpha = if db > self.smoothed_db[i] {
                attack
            } else {
                release
            };

            self.smoothed_db[i] += (db - self.smoothed_db[i]) * alpha;
        }

        if self.auto_gain {
            self.update_noise_floor();
        }

        let min_db = if self.auto_gain {
            self.floor_db
        } else {
            self.fixed_min_db
        };

        let max_db = self.fixed_max_db;
        let range_db = (max_db - min_db).max(1e-3);

        for i in 0..half {
            self.normalized_bins[i] = ((self.smoothed_db[i] - min_db) / range_db).clamp(0.0, 1.0);
        }

        let bands = group_into_log_bands(
            &self.normalized_bins,
            self.sample_rate,
            self.fft_size,
            self.num_bands,
        );

        for (disp, current) in self.display_bands.iter_mut().zip(bands) {
            if current > *disp {
                *disp = current
            } else {
                let alpha = self.decay_rate.clamp(0.001, 0.99);
                *disp += (current - *disp) * alpha;
            }
        }

        &self.display_bands
    }

    fn update_noise_floor(&mut self) {
        const FLOOR_MIN_DB: f32 = -100.0;
        const FLOOR_MAX_DB: f32 = -60.0;
        const FLOOR_RESPONSE: f32 = 0.01;

        self.db_scratch.clear();

        let start = if self.smoothed_db.len() > 1 { 1 } else { 0 };

        self.db_scratch
            .extend_from_slice(&self.smoothed_db[start..]);

        if self.db_scratch.is_empty() {
            return;
        }

        let idx = self.db_scratch.len() / 10;
        let (_, &mut target_floor, _) = self.db_scratch.select_nth_unstable_by(idx, |a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        });

        let target_floor = target_floor.clamp(FLOOR_MIN_DB, FLOOR_MAX_DB);

        self.floor_db += (target_floor - self.floor_db) * FLOOR_RESPONSE;
        self.floor_db = self.floor_db.clamp(FLOOR_MIN_DB, FLOOR_MAX_DB);
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
    if num_bands == 0 || magnitudes.is_empty() || fft_size == 0 || sample_rate == 0 {
        return vec![0.0; num_bands];
    }

    let min_freq = 30.0f32;
    let nyquist = sample_rate as f32 / 2.0;
    let max_freq = 16000.0f32.min(nyquist * 0.999);
    if max_freq <= min_freq {
        return vec![0.0; num_bands];
    }
    let half_size = magnitudes.len();
    let mut bands = vec![0.0f32; num_bands];
    let ratio = max_freq / min_freq;

    for (i, band) in bands.iter_mut().enumerate() {
        let freq_start = min_freq * ratio.powf(i as f32 / num_bands as f32);
        let freq_end = min_freq * ratio.powf((i + 1) as f32 / num_bands as f32);

        let bin_start =
            ((freq_start * fft_size as f32 / sample_rate as f32) as usize).min(half_size - 1);

        let bin_end = ((freq_end * fft_size as f32 / sample_rate as f32) as usize)
            .max(bin_start + 1)
            .min(half_size);

        let center_freq = (freq_start * freq_end).sqrt();

        let center_bin = center_freq * fft_size as f32 / sample_rate as f32;

        let val = if bin_end > bin_start + 1 {
            magnitudes[bin_start..bin_end]
                .iter()
                .copied()
                .fold(0.0f32, f32::max)
        } else {
            let lo = center_bin.floor() as usize;
            let lo = lo.min(half_size - 1);
            let hi = (lo + 1).min(half_size - 1);
            let frac = center_bin - lo as f32;
            magnitudes[lo] * (1.0 - frac) + magnitudes[hi] * frac
        };

        let boost = bass_boost_curve(center_freq);

        *band = (val * boost).clamp(0.0, 1.0);
    }
    bands
}

fn bass_boost_curve(freq: f32) -> f32 {
    const CUTOFF_HZ: f32 = 400.0;

    // Max Gain (ex: 1.05 = +5%)
    const MAX_BOOST: f32 = 1.00;

    if freq < CUTOFF_HZ {
        let t = (freq / CUTOFF_HZ).clamp(0.0, 1.0);

        MAX_BOOST - (MAX_BOOST - 1.0) * t
    } else {
        1.0
    }
}
