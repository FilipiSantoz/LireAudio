use super::Effect;
use std::f32::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Clone)]
pub struct VinylParams {
    pub enabled: Arc<AtomicBool>,
    pub amount: Arc<AtomicU32>,
    // pub crackle: Arc<AtomicU32>,
    pub hiss: Arc<AtomicU32>,
    pub wow: Arc<AtomicU32>,
    pub warmth: Arc<AtomicU32>,
}

impl Default for VinylParams {
    fn default() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            amount: Arc::new(AtomicU32::new(0.7f32.to_bits())),
            // crackle: Arc::new(AtomicU32::new(0.35f32.to_bits())),
            hiss: Arc::new(AtomicU32::new(0.25f32.to_bits())),
            wow: Arc::new(AtomicU32::new(0.45f32.to_bits())),
            warmth: Arc::new(AtomicU32::new(0.50f32.to_bits())),
        }
    }
}

pub struct VinylEffect {
    sample_rate: f32,
    rng: u32,

    delay_line: Vec<f32>,
    delay_mask: usize,
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

    current_amount: f32,
    // current_crackle: f32,
    current_hiss: f32,
    current_wow: f32,

    pub params: VinylParams,
}

impl VinylEffect {
    pub fn new(sample_rate: u32, params: VinylParams, seed: u32) -> Self {
        let delay_len = 4096usize.next_power_of_two();
        let delay_mask = delay_len - 1;

        let mut effect = Self {
            sample_rate: sample_rate as f32,
            rng: seed,

            delay_line: vec![0.0f32; delay_len],
            delay_mask,
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

            current_amount: 0.5,
            // current_crackle: 0.35,
            current_hiss: 0.40,
            current_wow: 0.45,

            params,
        };

        effect.update_coefficients();
        effect
    }

    #[inline(always)]
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / (u32::MAX as f32 + 1.0)) * 2.0 - 1.0
    }

    pub fn update_coefficients(&mut self) {
        let sr = self.sample_rate;
        let a = self.current_amount;
        let warmth = f32::from_bits(self.params.warmth.load(Ordering::Relaxed));

        self.drive = 1.0 + warmth * a * 2.0;
        let tanh_val = self.drive.tanh();
        self.inv_drive_tanh = if tanh_val.abs() > 1e-6 {
            1.0 / tanh_val
        } else {
            1.0
        };

        let fc = 14_000.0 - warmth * a * 8_000.0;
        self.lp_alpha = 1.0 - (-2.0 * PI * fc / sr).exp();

        let rc = 1.0 / (2.0 * PI * 40.0);
        let dt = 1.0 / sr;
        self.hp_a = rc / (rc + dt);

        self.noise_alpha = 1.0 - (-2.0 * PI * 4_000.0 / sr).exp();
    }

    #[inline(always)]
    fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, mu: f32) -> f32 {
        let c0 = y1;
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * mu + c2) * mu + c1) * mu + c0
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let sr = self.sample_rate;
        let a = self.current_amount;

        self.delay_line[self.write_pos] = input;

        let wow_s = (self.phase_wow * 2.0 * PI).sin() * self.current_wow * a * 0.0022;
        let flutter_s = (self.phase_flutter * 2.0 * PI).sin() * self.current_wow * a * 0.0003;

        self.phase_wow = (self.phase_wow + 0.55 / sr).rem_euclid(1.0);
        self.phase_flutter = (self.phase_flutter + 6.3 / sr).rem_euclid(1.0);

        let delay_samples = (0.003 + wow_s + flutter_s) * sr;
        let len_f = self.delay_line.len() as f32;
        let read_pos = (self.write_pos as f32 - delay_samples).rem_euclid(len_f);

        let i0 = read_pos.floor() as isize;
        let frac = read_pos - i0 as f32;
        let mask = self.delay_mask as isize;

        let y0 = self.delay_line[((i0 - 1) & mask) as usize];
        let y1 = self.delay_line[(i0 & mask) as usize];
        let y2 = self.delay_line[((i0 + 1) & mask) as usize];
        let y3 = self.delay_line[((i0 + 2) & mask) as usize];

        let mut x = Self::hermite(y0, y1, y2, y3, frac);

        self.write_pos = (self.write_pos + 1) & self.delay_mask;

        // Saturation
        x = (x * self.drive).tanh() * self.inv_drive_tanh;

        // Filters
        self.lp_state += self.lp_alpha * (x - self.lp_state);
        x = self.lp_state;

        let y = self.hp_a * (self.hp_state + x - self.hp_last_in);
        self.hp_state = y;
        self.hp_last_in = x;
        x = y;

        // Hiss
        let white = self.rand();
        self.noise_lp += self.noise_alpha * (white - self.noise_lp);
        x += self.noise_lp * self.current_hiss * a * 0.05;

        // crackle
        // let p = 0.00002 + self.current_crackle * a * 0.0003;
        // if (self.rand() + 1.0) * 0.5 < p {
        //     self.crackle_env = 0.08 + self.rand().abs() * 0.25;
        // }
        // x += self.crackle_env * self.rand();
        // self.crackle_env *= 0.996;

        x
    }
}

impl Effect for VinylEffect {
    fn process(&mut self, sample: f32) -> f32 {
        if !self.params.enabled.load(Ordering::Relaxed) {
            return sample;
        }
        self.process(sample)
    }

    fn process_block(&mut self, data: &mut [f32]) {
        if !self.params.enabled.load(Ordering::Relaxed) {
            return;
        }

        self.current_amount = f32::from_bits(self.params.amount.load(Ordering::Relaxed));
        // self.current_crackle = f32::from_bits(self.params.crackle.load(Ordering::Relaxed));
        self.current_hiss = f32::from_bits(self.params.hiss.load(Ordering::Relaxed));
        self.current_wow = f32::from_bits(self.params.wow.load(Ordering::Relaxed));

        self.update_coefficients();

        for s in data.iter_mut() {
            *s = self.process(*s);
        }
    }

    fn prepare(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate as f32;
        self.delay_line.fill(0.0);
        self.update_coefficients();
    }
}
