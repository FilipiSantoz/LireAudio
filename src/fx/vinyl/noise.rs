use super::dsp::{Biquad, OnePoleHp, OnePoleLp, Xorshift32};
use std::f32::consts::PI;

// 6-pole Gardner/Kellet filter approximation for pink noise (-3 dB/octave).
#[derive(Default, Clone, Copy)]
pub struct PinkNoise {
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
}

impl PinkNoise {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn process(&mut self, white: f32) -> f32 {
        self.b0 = 0.99886 * self.b0 + white * 0.0555179;
        self.b1 = 0.99332 * self.b1 + white * 0.0750759;
        self.b2 = 0.96900 * self.b2 + white * 0.153_852;
        self.b3 = 0.86650 * self.b3 + white * 0.3104856;
        self.b4 = 0.55000 * self.b4 + white * 0.5329522;
        self.b5 = -0.76160 * self.b5 - white * 0.0168980;

        let pink = self.b0 + self.b1 + self.b2 + self.b3 + self.b4 + self.b5 + white * 0.5362;
        pink * 0.11
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// Leaky integrator driven by dedicated pink noise to produce Brown noise (-6 dB/octave).
#[derive(Default, Clone, Copy)]
pub struct BrownNoise {
    pink: PinkNoise,
    state: f32,
}

impl BrownNoise {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn process(&mut self, white: f32) -> f32 {
        let pink_val = self.pink.process(white);
        self.state = (self.state * 0.983) + (pink_val * 0.026);
        self.state
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.pink.reset();
        self.state = 0.0;
    }
}

// Needle Drop transient (the low mechanical thump when needle touches groove).
#[derive(Default, Clone, Copy)]
pub struct NeedleDropTransient {
    phase: f32,
    env: f32,
    decay: f32,
    freq: f32,
}

impl NeedleDropTransient {
    pub fn new(sample_rate: f32) -> Self {
        let mut drop = Self {
            phase: 0.0,
            env: 0.0,
            decay: 0.0,
            freq: 62.0,
        };
        drop.set_sample_rate(sample_rate);
        drop
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.decay = (-1.0 / (0.085 * sample_rate)).exp();
        }
    }

    pub fn trigger(&mut self, intensity: f32) {
        self.env = intensity.clamp(0.0, 1.0);
        self.phase = 0.0;
    }

    #[inline(always)]
    pub fn process(&mut self, sample_rate: f32) -> f32 {
        if self.env < 1e-5 || sample_rate <= 0.0 {
            return 0.0;
        }
        let out = (self.phase * 2.0 * PI).sin() * self.env;
        self.phase = (self.phase + self.freq / sample_rate).rem_euclid(1.0);
        self.env *= self.decay;
        out
    }
}

// Surface noise / Hiss generator.
//
// Uses modulated multiplication of body and air noise components to create
// an vinyl texture
pub struct SurfaceNoise {
    pink_body: PinkNoise,
    pink_air: PinkNoise,

    body_hp: OnePoleHp,
    body_lp: OnePoleLp,

    air_hp1: OnePoleHp,
    air_hp2: OnePoleHp,
    air_lp1: OnePoleLp,
    air_lp2: OnePoleLp,

    post_hp: OnePoleHp,
    hf_cut: OnePoleLp,

    lfo_phase: f32,
}

impl SurfaceNoise {
    pub fn new(sample_rate: f32) -> Self {
        let mut sn = Self {
            pink_body: PinkNoise::new(),
            pink_air: PinkNoise::new(),

            body_hp: OnePoleHp::new(800.0, sample_rate),
            body_lp: OnePoleLp::new(4500.0, sample_rate),

            air_hp1: OnePoleHp::new(2800.0, sample_rate),
            air_hp2: OnePoleHp::new(3400.0, sample_rate),
            air_lp1: OnePoleLp::new(8800.0, sample_rate),
            air_lp2: OnePoleLp::new(9800.0, sample_rate),

            post_hp: OnePoleHp::new(900.0, sample_rate),
            hf_cut: OnePoleLp::new(11200.0, sample_rate),

            lfo_phase: 0.0,
        };
        sn.reset(sample_rate);
        sn
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.pink_body.reset();
        self.pink_air.reset();

        self.body_hp = OnePoleHp::new(800.0, sample_rate);
        self.body_lp = OnePoleLp::new(4500.0, sample_rate);

        self.air_hp1 = OnePoleHp::new(2800.0, sample_rate);
        self.air_hp2 = OnePoleHp::new(3400.0, sample_rate);
        self.air_lp1 = OnePoleLp::new(8800.0, sample_rate);
        self.air_lp2 = OnePoleLp::new(9800.0, sample_rate);

        self.post_hp = OnePoleHp::new(900.0, sample_rate);
        self.hf_cut = OnePoleLp::new(11200.0, sample_rate);

        self.lfo_phase = 0.0;
    }

    #[inline(always)]
    pub fn process(&mut self, rng: &mut Xorshift32, sample_rate: f32) -> f32 {
        let white1 = rng.rand_gaussian();
        let white2 = rng.rand_gaussian();

        let pink_b = self.pink_body.process(white1);
        let body = self.body_lp.process(self.body_hp.process(pink_b));

        let pink_a = self.pink_air.process(white2 * 0.7 + white1 * 0.3);
        let air_hp = self.air_hp2.process(self.air_hp1.process(pink_a));
        let air = self.air_lp2.process(self.air_lp1.process(air_hp));

        // Slow turntable rotation modulation
        self.lfo_phase += 0.35 / sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo = 0.5 * ((self.lfo_phase * 2.0 * PI).sin() + 1.0);
        let motion = 0.78 + lfo * 0.22;

        // Modulated texture multiplication: carrier `air` + ring-modulated `body * air`,
        // followed by `post_hp` (900 Hz) to strip away the low-frequency difference products.
        let mod_grain = air * (0.55 + (body * 9.5).clamp(-0.85, 0.85));
        let clean_hiss = self.post_hp.process(mod_grain);
        let shaped = self.hf_cut.process(clean_hiss);

        shaped * 0.86 * motion
    }
}

// Low-frequency mechanical groove friction and motor hum.
pub struct RumbleNoise {
    brown: BrownNoise,
    rumble_biquad: Biquad,
    friction_biquad: Biquad,
    needle_drop: NeedleDropTransient,

    hum_phase: f32,
    hum_freq: f32,
    duck_env: f32,
    duck_attack_alpha: f32,
    duck_release_alpha: f32,
}

impl RumbleNoise {
    pub fn new(sample_rate: f32, hum_freq: f32) -> Self {
        let mut r = Self {
            brown: BrownNoise::new(),
            rumble_biquad: Biquad::bandpass(52.0, 0.8, sample_rate),
            friction_biquad: Biquad::bandpass(240.0, 0.5, sample_rate),
            needle_drop: NeedleDropTransient::new(sample_rate),
            hum_phase: 0.0,
            hum_freq,
            duck_env: 0.0,
            duck_attack_alpha: 0.0,
            duck_release_alpha: 0.0,
        };
        r.reset(sample_rate);
        r
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.brown.reset();
        self.rumble_biquad = Biquad::bandpass(52.0, 0.8, sample_rate);
        self.friction_biquad = Biquad::bandpass(240.0, 0.5, sample_rate);
        self.needle_drop.set_sample_rate(sample_rate);
        self.hum_phase = 0.0;
        self.duck_env = 0.0;

        if sample_rate > 0.0 {
            self.duck_attack_alpha = 1.0 - (-1.0 / (0.015 * sample_rate)).exp();
            self.duck_release_alpha = 1.0 - (-1.0 / (0.180 * sample_rate)).exp();
        }
    }

    pub fn trigger_needle_drop(&mut self, intensity: f32) {
        self.needle_drop.trigger(intensity);
    }

    #[inline(always)]
    pub fn update_ducking(&mut self, dry_signal: f32) -> f32 {
        let target = dry_signal * dry_signal;
        let alpha = if target > self.duck_env {
            self.duck_attack_alpha
        } else {
            self.duck_release_alpha
        };
        self.duck_env += alpha * (target - self.duck_env);
        let level = self.duck_env.sqrt();
        1.0 / (1.0 + level * 1.4)
    }

    #[inline(always)]
    pub fn process(&mut self, rng: &mut Xorshift32, ducking: f32, sample_rate: f32) -> (f32, f32) {
        let white = rng.rand_gaussian();
        let brown_val = self.brown.process(white);

        // Motor electrical hum with fundamental and 2nd harmonic
        self.hum_phase = (self.hum_phase + self.hum_freq / sample_rate).rem_euclid(1.0);
        let hum_angle = self.hum_phase * 2.0 * PI;
        let motor_hum = hum_angle.sin() * 0.008 + (hum_angle * 2.0).sin() * 0.003;

        let rumble_in = brown_val * 0.85 + motor_hum * 1.15;
        let rumble_core = self.rumble_biquad.process(rumble_in);
        let friction = self.friction_biquad.process(rumble_in);

        let continuous = (friction * 0.72 + rumble_core * 0.72) * ducking;
        let landing = self.needle_drop.process(sample_rate);

        (continuous, landing)
    }
}
