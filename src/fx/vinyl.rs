use super::Effect;
use std::f32::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Clone)]
pub struct VinylParams {
    pub enabled: Arc<AtomicBool>,
    pub amount: Arc<AtomicU32>,
    pub crackle: Arc<AtomicU32>,
    pub hiss: Arc<AtomicU32>,
    pub texture: Arc<AtomicU32>,
    pub wow: Arc<AtomicU32>,
    pub warmth: Arc<AtomicU32>,
    pub needle: Arc<AtomicU32>,
}

impl Default for VinylParams {
    fn default() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            amount: Arc::new(AtomicU32::new(0.70f32.to_bits())),
            crackle: Arc::new(AtomicU32::new(0.35f32.to_bits())),
            hiss: Arc::new(AtomicU32::new(0.25f32.to_bits())),
            texture: Arc::new(AtomicU32::new(0.25f32.to_bits())),
            wow: Arc::new(AtomicU32::new(0.45f32.to_bits())),
            warmth: Arc::new(AtomicU32::new(0.50f32.to_bits())),
            needle: Arc::new(AtomicU32::new(0.40f32.to_bits())),
        }
    }
}

#[derive(Default)]
struct PinkNoiseFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
}

impl PinkNoiseFilter {
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn process(&mut self, white: f32) -> f32 {
        self.b0 = 0.99886 * self.b0 + white * 0.0555179;
        self.b1 = 0.99332 * self.b1 + white * 0.0750759;
        self.b2 = 0.96900 * self.b2 + white * 0.153_852;
        self.b3 = 0.86650 * self.b3 + white * 0.3104856;
        self.b4 = 0.55000 * self.b4 + white * 0.5329522;
        self.b5 = -0.7616 * self.b5 - white * 0.0168980;

        let pink = self.b0 + self.b1 + self.b2 + self.b3 + self.b4 + self.b5 + white * 0.5362;
        pink * 0.11
    }
}

#[derive(Default, Clone, Copy)]
struct NeedleRumbleFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl NeedleRumbleFilter {
    fn new(sr: f32, freq: f32, q: f32) -> Self {
        let mut filter = Self::default();
        filter.update(sr, freq, q);
        filter
    }

    fn update(&mut self, sr: f32, freq: f32, q: f32) {
        let w0 = 2.0 * PI * (freq / sr);
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;

        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let out = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * out + self.z2;
        self.z2 = self.b2 * x - self.a2 * out;
        out
    }
}

#[derive(Default)]
struct NeedleDropTransient {
    phase: f32,
    env: f32,
    decay: f32,
    freq: f32,
}

impl NeedleDropTransient {
    fn new(sr: f32) -> Self {
        Self {
            phase: 0.0,
            env: 0.0,
            decay: (-1.0 / (0.085 * sr)).exp(),
            freq: 62.0,
        }
    }

    fn update_sr(&mut self, sr: f32) {
        self.decay = (-1.0 / (0.085 * sr)).exp();
    }

    fn trigger(&mut self, intensity: f32) {
        self.env = intensity;
        self.phase = 0.0;
    }

    #[inline(always)]
    fn process(&mut self, sr: f32) -> f32 {
        if self.env < 1e-5 {
            return 0.0;
        }

        let out = (self.phase * 2.0 * PI).sin() * self.env;
        self.phase = (self.phase + self.freq / sr).rem_euclid(1.0);
        self.env *= self.decay;
        out
    }
}

#[derive(Default, Clone, Copy)]
struct CrackleBurst {
    env: f32,
    decay: f32,

    hp_state: f32,
    hp_last: f32,
    lp1_state: f32,
    lp2_state: f32,
    lp3_state: f32,

    hp_alpha: f32,
    lp_alpha: f32,

    // Broad 3-pole surface response + a gentle 5.2 kHz presence resonance.
    spark_filter: NeedleRumbleFilter,
}

impl CrackleBurst {
    fn new(sr: f32) -> Self {
        let mut burst = Self::default();
        burst.update(sr);
        burst
    }

    fn update(&mut self, sr: f32) {
        self.decay = (-1.0 / (0.00085 * sr)).exp();

        // Keep the click broad, but remove DC/subsonics.
        let hp_rc = 1.0 / (2.0 * PI * 300.0);
        let dt = 1.0 / sr;
        self.hp_alpha = hp_rc / (hp_rc + dt);

        // Three identical one-pole LP stages reproduce the steep high-end
        // roll-off measured in the reference above ~10 kHz.
        self.lp_alpha = 1.0 - (-2.0 * PI * 3_500.0 / sr).exp();
        self.spark_filter.update(sr, 4_200.0, 0.65);
    }

    #[inline(always)]
    fn trigger(&mut self, amplitude: f32) {
        self.env = amplitude;
    }

    #[inline(always)]
    fn active(&self) -> bool {
        self.env > 1e-6
    }

    #[inline(always)]
    fn process(&mut self, white: f32) -> f32 {
        if self.env <= 1e-6 {
            return 0.0;
        }

        let input = white * self.env;

        // First remove DC / very low-frequency content.
        let hp = self.hp_alpha * (self.hp_state + input - self.hp_last);
        self.hp_state = hp;
        self.hp_last = input;

        // Then shape the transient with a steep vinyl-like HF roll-off.
        self.lp1_state += self.lp_alpha * (hp - self.lp1_state);
        self.lp2_state += self.lp_alpha * (self.lp1_state - self.lp2_state);
        self.lp3_state += self.lp_alpha * (self.lp2_state - self.lp3_state);

        // A broad 4-8 kHz component is characteristic of the larger clicks.
        let spark = self.spark_filter.process(input);

        let out = self.lp3_state + spark * 0.40;
        self.env *= self.decay;
        out
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

    crackle: CrackleBurst,
    lp_state: f32,
    hp_state: f32,
    hp_last_in: f32,

    lp_alpha: f32,
    hp_a: f32,

    warm_pre_lp_alpha: f32,
    warm_post_lp_alpha: f32,
    warm_pre_lp_state: f32,
    warm_post_lp_state: f32,
    warm_drive: f32,
    warm_mix: f32,
    warm_bias: f32,

    lfo_phase: f32,
    hum_phase: f32,
    body_hp_alpha: f32,
    body_lp_alpha: f32,
    air_hp_alpha: f32,
    air_lp_alpha: f32,

    body_hp_state: f32,
    body_lp_state: f32,
    body_last: f32,
    air_hp_state: f32,
    air_lp_state: f32,
    air_lp_state_2: f32,
    air_lp_state_3: f32,
    air_last: f32,
    hiss_dc_alpha: f32,
    hiss_dc_state: f32,

    brown_state: f32,
    rumble_filter: NeedleRumbleFilter,
    rumble_friction: NeedleRumbleFilter,
    needle_drop: NeedleDropTransient,
    duck_env: f32,
    duck_attack_alpha: f32,
    duck_release_alpha: f32,

    current_amount: f32,
    current_crackle: f32,
    current_hiss: f32,
    current_texture: f32,
    current_wow: f32,
    current_needle: f32,

    pink_hiss_filter: PinkNoiseFilter,
    pink_brown_filter: PinkNoiseFilter,
    pub params: VinylParams,
}

impl VinylEffect {
    pub fn new(sample_rate: u32, params: VinylParams, seed: u32) -> Self {
        let delay_len = 4096usize.next_power_of_two();
        let delay_mask = delay_len - 1;
        let sr_f32 = sample_rate as f32;

        let mut effect = Self {
            sample_rate: sr_f32,
            rng: seed,

            delay_line: vec![0.0f32; delay_len],
            delay_mask,
            write_pos: 0,
            phase_wow: 0.0,
            phase_flutter: 0.0,

            crackle: CrackleBurst::new(sr_f32),
            lp_state: 0.0,
            hp_state: 0.0,
            hp_last_in: 0.0,

            lp_alpha: 0.0,
            hp_a: 0.0,

            warm_pre_lp_alpha: 0.0,
            warm_post_lp_alpha: 0.0,
            warm_pre_lp_state: 0.0,
            warm_post_lp_state: 0.0,
            warm_drive: 1.0,
            warm_mix: 0.0,
            warm_bias: 0.0,

            lfo_phase: 0.0,
            hum_phase: 0.0,
            body_hp_alpha: 0.0,
            body_lp_alpha: 0.0,
            air_hp_alpha: 0.0,
            air_lp_alpha: 0.0,

            body_hp_state: 0.0,
            body_lp_state: 0.0,
            body_last: 0.0,
            air_hp_state: 0.0,
            air_lp_state: 0.0,
            air_lp_state_2: 0.0,
            air_lp_state_3: 0.0,
            air_last: 0.0,
            hiss_dc_alpha: 0.0,
            hiss_dc_state: 0.0,

            brown_state: 0.0,
            rumble_filter: NeedleRumbleFilter::new(sr_f32, 70.0, 1.2),
            rumble_friction: NeedleRumbleFilter::new(sr_f32, 170.0, 0.85),
            needle_drop: NeedleDropTransient::new(sr_f32),
            duck_env: 0.0,
            duck_attack_alpha: 0.0,
            duck_release_alpha: 0.0,

            current_amount: 0.70,
            current_crackle: 0.35,
            current_hiss: 0.25,
            current_texture: 0.25,
            current_wow: 0.45,
            current_needle: 0.40,

            pink_hiss_filter: PinkNoiseFilter::new(),
            pink_brown_filter: PinkNoiseFilter::new(),
            params,
        };

        effect.update_coefficients();
        effect
    }

    pub fn trigger_needle_drop(&mut self) {
        self.needle_drop.trigger(1.0);
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

    #[inline(always)]
    fn rand_gaussian(&mut self) -> f32 {
        let mut sum = 0.0;
        for _ in 0..3 {
            let mut x = self.rng;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.rng = x;
            sum += (x as f32 / (u32::MAX as f32 + 1.0)) * 2.0 - 1.0;
        }
        sum * 0.577
    }

    #[inline(always)]
    fn brown_noise(&mut self) -> f32 {
        let white = self.rand_gaussian();
        let pink = self.pink_brown_filter.process(white);
        self.brown_state = (self.brown_state * 0.983) + (pink * 0.026);
        self.brown_state
    }

    pub fn update_coefficients(&mut self) {
        let sr = self.sample_rate;
        let a = self.current_amount.clamp(0.0, 1.0);
        let warmth = f32::from_bits(self.params.warmth.load(Ordering::Relaxed)).clamp(0.0, 1.0);
        let dt = 1.0 / sr;

        let w = (warmth * a).clamp(0.0, 1.0);
        let w2 = w * w;

        let fc = 18_000.0 - (a * 3_000.0);
        self.lp_alpha = 1.0 - (-2.0 * PI * fc / sr).exp();

        let warm_pre_fc = 18_000.0 - (w2 * 7_000.0);
        let warm_post_fc = 18_000.0 - (w2 * 6_000.0);

        self.warm_pre_lp_alpha = 1.0 - (-2.0 * PI * warm_pre_fc / sr).exp();
        self.warm_post_lp_alpha = 1.0 - (-2.0 * PI * warm_post_fc / sr).exp();

        self.warm_drive = 1.0 + (w2 * 0.75);
        self.warm_mix = 0.20 * w2;
        self.warm_bias = 0.02 * w2;

        let rc = 1.0 / (2.0 * PI * 40.0);
        self.hp_a = rc / (rc + dt);

        let body_rc = 1.0 / (2.0 * PI * 600.0);
        self.body_hp_alpha = body_rc / (body_rc + dt);
        self.body_lp_alpha = 1.0 - (-2.0 * PI * 5_500.0 / sr).exp();

        let air_rc = 1.0 / (2.0 * PI * 3_400.0);
        self.air_hp_alpha = air_rc / (air_rc + dt);
        self.air_lp_alpha = 1.0 - (-2.0 * PI * 6_500.0 / sr).exp();
        self.hiss_dc_alpha = 1.0 - (-2.0 * PI * 800.0 / sr).exp();

        self.crackle.update(sr);

        self.rumble_filter.update(sr, 52.0, 0.8);
        self.rumble_friction.update(sr, 240.0, 0.5);
        self.needle_drop.update_sr(sr);
        self.duck_attack_alpha = 1.0 - (-1.0 / (0.015 * sr)).exp();
        self.duck_release_alpha = 1.0 - (-1.0 / (0.180 * sr)).exp();
    }

    #[inline(always)]
    fn velvet_saturate(x: f32, drive: f32, bias: f32) -> f32 {
        let biased = x.mul_add(drive, bias);
        let bias_tanh = bias.tanh();
        let linear_gain = drive * (1.0 - bias_tanh * bias_tanh);

        let y = biased.tanh() - bias_tanh;
        y / linear_gain.max(1e-6)
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

        let wow_s = (self.phase_wow * 2.0 * PI).sin() * self.current_wow * a * 0.0012;
        let flutter_s = (self.phase_flutter * 2.0 * PI).sin() * self.current_wow * a * 0.0014;

        self.phase_wow = (self.phase_wow + 0.5555 / sr).rem_euclid(1.0);
        self.phase_flutter = (self.phase_flutter + 4.6 / sr).rem_euclid(1.0);

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

        let x_dry = Self::hermite(y0, y1, y2, y3, frac);
        self.write_pos = (self.write_pos + 1) & self.delay_mask;

        // 2. Aquecimento e Saturação (Warmth)
        self.warm_pre_lp_state += self.warm_pre_lp_alpha * (x_dry - self.warm_pre_lp_state);
        let warm_input = self.warm_pre_lp_state;

        let warm_sat = Self::velvet_saturate(warm_input, self.warm_drive, self.warm_bias);

        self.warm_post_lp_state += self.warm_post_lp_alpha * (warm_sat - self.warm_post_lp_state);
        let warm = self.warm_post_lp_state;

        let mut x = x_dry + (warm - x_dry) * self.warm_mix;

        // 3. Filtros do Vinil (Corte de extremidades)
        self.lp_state += self.lp_alpha * (x - self.lp_state);
        x = self.lp_state;

        let y = self.hp_a * (self.hp_state + x - self.hp_last_in);
        self.hp_state = y;
        self.hp_last_in = x;
        x = y;

        // 4. Hiss (Ar e Médios da fita/eletrônica)
        let white = self.rand_gaussian();
        let pink = self.pink_hiss_filter.process(white);
        let body_brown = self.brown_noise();
        let pink_brown = self.pink_hiss_filter.process(body_brown * 0.84);
        let body_hp = self.body_hp_alpha * (self.body_hp_state + pink - self.body_last);
        self.body_hp_state = body_hp;
        self.body_last = pink;
        self.body_lp_state += self.body_lp_alpha * (body_hp - self.body_lp_state);
        let main_body = self.body_lp_state;

        let air_hp = self.air_hp_alpha * (self.air_hp_state + pink_brown - self.air_last);
        self.air_hp_state = air_hp;
        self.air_last = pink_brown;
        self.air_lp_state += self.air_lp_alpha * (air_hp - self.air_lp_state);
        self.air_lp_state_2 += self.air_lp_alpha * (self.air_lp_state - self.air_lp_state_2);
        self.air_lp_state_3 += self.air_lp_alpha * (self.air_lp_state_2 - self.air_lp_state_3);
        let main_air = self.air_lp_state_3;

        self.lfo_phase += 0.18 / sr;
        let lfo_raw = (self.lfo_phase * 2.0 * PI).sin();
        let lfo = 0.5 * (lfo_raw + 1.0);
        let motion = 0.214 + lfo * 0.075;

        let raw_hiss = (main_body * 2.25) * (main_air * 2.50);
        self.hiss_dc_state += self.hiss_dc_alpha * (raw_hiss - self.hiss_dc_state);
        let main_hiss = raw_hiss - self.hiss_dc_state;

        x += (main_hiss * 3.2) * self.current_hiss * a * motion;

        // 5. Crackle / dust / pops
        let crackle_rate = self.current_crackle * a * 30.0;
        let crackle_probability = crackle_rate / sr;

        if self.rand().mul_add(0.5, 0.5) < crackle_probability {
            let u = self.rand().abs();

            // Heavy-tailed amplitude distribution:
            // many small dust clicks + a few much larger pops.
            let amplitude = 0.020 + 0.36 * u.powi(4);
            self.crackle.trigger(amplitude);
        }

        if self.crackle.active() {
            let crackle_white = self.rand_gaussian();
            x += self.crackle.process(crackle_white * 3.44);
        }

        let target_duck = x_dry * x_dry;
        let d_alpha = if target_duck > self.duck_env {
            self.duck_attack_alpha
        } else {
            self.duck_release_alpha
        };
        self.duck_env += d_alpha * (target_duck - self.duck_env);
        let level = self.duck_env.sqrt();
        let ducking = 1.0 / (1.0 + level * 1.2);

        // Brown noise friction
        let brown = self.brown_noise();
        self.hum_phase = (self.hum_phase + 50.0 / sr).rem_euclid(1.0);
        let hum = self.hum_phase * 2.0 * PI;
        let motor_hum = hum.sin() * 0.008 + (hum * 5.0).sin() * 0.002;

        let rumble_input = brown * 0.8 + motor_hum * 1.2;
        let rumble_core = self.rumble_filter.process(rumble_input);
        let friction = self.rumble_friction.process(rumble_input);
        let main_texture = friction * 0.24 + rumble_core * 0.82;
        let continuous_groove = main_texture * (self.current_texture * a) * motion * ducking;

        let landing_impact = self.needle_drop.process(sr) * (self.current_needle * a * 0.16);

        x += continuous_groove + landing_impact;

        x * 1.1
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
        self.current_crackle = f32::from_bits(self.params.crackle.load(Ordering::Relaxed));
        self.current_hiss = f32::from_bits(self.params.hiss.load(Ordering::Relaxed));
        self.current_texture = f32::from_bits(self.params.texture.load(Ordering::Relaxed));
        self.current_wow = f32::from_bits(self.params.wow.load(Ordering::Relaxed));
        self.current_needle = f32::from_bits(self.params.needle.load(Ordering::Relaxed));

        self.update_coefficients();

        for s in data.iter_mut() {
            *s = self.process(*s);
        }
    }

    fn prepare(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate as f32;
        self.delay_line.fill(0.0);
        self.update_coefficients();
        self.needle_drop.trigger(1.0);
    }
}
