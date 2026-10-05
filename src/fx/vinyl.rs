use crate::fx::fx_chain::Effect;
use std::f32::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct VinylEffect {
    sample_rate: f32,
    rng: u32,

    delay_line: Vec<f32>,
    write_pos: usize,
    phase_wow: f32,
    phase_flutter: f32,

    // crackle_env: f32,
    noise_lp: f32,
    lp_state: f32,
    hp_state: f32,
    hp_last_in: f32,

    lp_alpha: f32,
    hp_a: f32,
    noise_alpha: f32,
    drive: f32,
    inv_drive_tanh: f32,

    amount_shared: Arc<AtomicU32>,
    enabled_shared: Arc<AtomicBool>,
    last_amount: f32,
    pub crackle: f32,
    pub hiss: f32,
    pub wow: f32,
    pub warmth: f32,
}

impl VinylEffect {
    pub fn new(sample_rate: u32, amount: Arc<AtomicU32>, enabled: Arc<AtomicBool>) -> Self {
        let mut effect = Self {
            sample_rate: sample_rate as f32,
            rng: 0x9E3779B9,
            delay_line: vec![0.0; 4096],
            write_pos: 0,
            phase_wow: 0.0,
            phase_flutter: 0.0,
            // crackle_env: 0.0,
            noise_lp: 0.0,
            lp_state: 0.0,
            hp_state: 0.0,
            hp_last_in: 0.0,

            lp_alpha: 0.0,
            hp_a: 0.0,
            noise_alpha: 0.0,
            drive: 1.0,
            inv_drive_tanh: 1.0,

            amount_shared: amount,
            enabled_shared: enabled,
            last_amount: -1.0,
            crackle: 0.4,
            hiss: 0.35,
            wow: 0.5,
            warmth: 0.5,
        };
        effect.update_coefficients();
        effect
    }
    #[inline]
    fn rand(&mut self) -> f32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0 // [-1, 1)
    }

    pub fn update_coefficients(&mut self) {
        let sr = self.sample_rate;
        let a = self.amount();

        // Drive & Saturation
        self.drive = 1.0 + self.warmth * a * 2.0;
        self.inv_drive_tanh = 1.0 / self.drive.tanh();

        // Lowpass
        let fc = 14_000.0 - self.warmth * a * 8_000.0;
        self.lp_alpha = 1.0 - (-2.0 * PI * fc / sr).exp();

        // Highpass (40 Hz)
        let rc = 1.0 / (2.0 * PI * 40.0);
        let dt = 1.0 / sr;
        self.hp_a = rc / (rc + dt);

        // Hiss filter (4 kHz)
        self.noise_alpha = 1.0 - (-2.0 * PI * 4_000.0 / sr).exp();
    }
    #[inline]
    fn amount(&self) -> f32 {
        f32::from_bits(self.amount_shared.load(Ordering::Relaxed))
    }

    #[inline]
    fn is_enabled(&self) -> bool {
        self.enabled_shared.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let sr = self.sample_rate;
        let a = self.amount();

        // 1. Wow & Flutter
        self.delay_line[self.write_pos] = input;
        let wow = (self.phase_wow * 2.0 * PI).sin() * self.wow * a * 0.0022 * sr;
        let flutter = (self.phase_flutter * 2.0 * PI).sin() * self.wow * a * 0.0003 * sr;

        self.phase_wow = (self.phase_wow + 0.55 / sr) % 1.0;
        self.phase_flutter = (self.phase_flutter + 6.3 / sr) % 1.0;

        let delay = 0.003 * sr + wow + flutter;
        let len = self.delay_line.len();
        let read = (self.write_pos as f32 - delay).rem_euclid(len as f32);
        let i0 = read as usize;
        let frac = read - i0 as f32;
        let i1 = (i0 + 1) % len;
        let mut x = self.delay_line[i0] * (1.0 - frac) + self.delay_line[i1] * frac;

        self.write_pos = (self.write_pos + 1) % len;

        // Saturarion
        x = (x * self.drive).tanh() * self.inv_drive_tanh;

        // Low-Pass
        self.lp_state += self.lp_alpha * (x - self.lp_state);
        x = self.lp_state;

        // High-Pass
        let y = self.hp_a * (self.hp_state + x - self.hp_last_in);
        self.hp_state = y;
        self.hp_last_in = x;
        x = y;

        // Hiss
        let white = self.rand();
        self.noise_lp += self.noise_alpha * (white - self.noise_lp);
        x += self.noise_lp * self.hiss * a * 0.06;

        // Crackle
        // let p = 0.00002 + self.crackle * a * 0.0003;
        // if (self.rand() + 1.0) * 0.5 < p {
        //     self.crackle_env = 0.08 + self.rand().abs() * 0.25;
        // }
        // x += self.crackle_env * self.rand();
        // self.crackle_env *= 0.996;
        //
        x
    }
}

impl Effect for VinylEffect {
    fn process(&mut self, sample: f32) -> f32 {
        if !self.is_enabled() {
            return sample;
        }
        self.process(sample)
    }

    fn process_block(&mut self, data: &mut [f32]) {
        if !self.is_enabled() {
            return;
        }

        let current_a = self.amount();
        if (current_a - self.last_amount).abs() > 1e-3 {
            self.last_amount = current_a;
            self.update_coefficients();
        }
        for s in data.iter_mut() {
            *s = self.process(*s)
        }
    }

    fn prepare(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate as f32;
        self.delay_line.fill(0.0);
        self.update_coefficients(); // Updates to the new SampleRate 
    }
}
