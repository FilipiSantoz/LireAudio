use crate::pipeline::HOP_SIZE;

// Smoothing
const ATTACK_TAU: f32 = 0.018;
const RELEASE_TAU: f32 = 0.090;
const STRONG_RISE: f32 = 0.12;
const NOISE_FLOOR: f32 = 0.01;

pub const BASS_FREQ_LIMIT: f32 = 250.0;
pub const MID_FREQ_LIMIT: f32 = 3500.0;
pub const HIGH_FREQ_LIMIT: f32 = 16000.0;

// Weight in '%'
const BASS_ENERGY_WEIGHT: f32 = 0.96;
const MID_ENERGY_WEIGHT: f32 = 0.80;
const HIGH_ENERGY_WEIGHT: f32 = 0.74;

// Gamma per band
// Higher Value = -Energy
const BASS_GAMMA: f32 = 1.02;
const MID_GAMMA: f32 = 1.20;
const HIGH_GAMMA: f32 = 1.25;

struct BandProfile {
    weight: f32,
    gamma: f32,
}

pub struct BandSmoother {
    attack_k: f32,
    release_k: f32,
    profiles: Vec<BandProfile>,
}

impl BandSmoother {
    pub fn new(center_freqs: &[f32], sample_rate: u32) -> Self {
        let hop_dt = if sample_rate > 0 {
            HOP_SIZE as f32 / sample_rate as f32
        } else {
            0.0053
        };
        let attack_k = 1.0 - (-hop_dt / ATTACK_TAU).exp();
        let release_k = 1.0 - (-hop_dt / RELEASE_TAU).exp();

        let profiles = center_freqs
            .iter()
            .map(|&freq| {
                let (weight, gamma) = band_energy_profile(freq);
                BandProfile { weight, gamma }
            })
            .collect();

        Self {
            attack_k,
            release_k,
            profiles,
        }
    }

    pub fn update_band(&self, band_idx: usize, disp: &mut f32, current: f32) {
        let profile = &self.profiles[band_idx];
        let weighted = (current * profile.weight).clamp(0.0, 1.0);

        let gated = if weighted <= NOISE_FLOOR {
            0.0
        } else {
            (weighted - NOISE_FLOOR) / (1.0 - NOISE_FLOOR)
        };

        let target = gated.powf(profile.gamma);
        let delta = target - *disp;

        if delta >= STRONG_RISE {
            *disp = target;
        } else if delta > 0.0 {
            let t = smoothstep(0.0, STRONG_RISE * 2.0, delta);
            let k = lerp(self.attack_k, 1.0, t);
            *disp += delta * k;
        } else {
            *disp += delta * self.release_k;
        }
    }
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
