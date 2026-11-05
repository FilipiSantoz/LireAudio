use super::dsp::{Biquad, OnePoleHp, OnePoleLp, Xorshift32};

/// Single crackle / pop transient synthesizer.
#[derive(Default, Clone, Copy)]
pub struct CrackleBurst {
    env: f32,
    decay: f32,

    hp: OnePoleHp,
    lp1: OnePoleLp,
    lp2: OnePoleLp,
    lp3: OnePoleLp,
    spark_filter: Biquad,
}

impl CrackleBurst {
    pub fn new(sample_rate: f32) -> Self {
        let mut cb = Self::default();
        cb.reset(sample_rate);
        cb
    }

    pub fn reset(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            // Short, crisp needle impulse (~0.45 ms decay)
            self.decay = (-1.0 / (0.00045 * sample_rate)).exp();
            self.hp = OnePoleHp::new(800.0, sample_rate);
            self.lp1 = OnePoleLp::new(5000.0, sample_rate);
            self.lp2 = OnePoleLp::new(5600.0, sample_rate);
            self.lp3 = OnePoleLp::new(6200.0, sample_rate);
            self.spark_filter = Biquad::bandpass(4200.0, 0.75, sample_rate);
        }
        self.env = 0.0;
    }

    #[inline(always)]
    pub fn trigger(&mut self, amplitude: f32) {
        self.env = amplitude;
    }

    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.env > 1e-6
    }

    #[inline(always)]
    pub fn process(&mut self, white: f32) -> f32 {
        if self.env <= 1e-6 {
            return 0.0;
        }

        let input = white * self.env;
        let hp_val = self.hp.process(input);
        let lp_val = self.lp3.process(self.lp2.process(self.lp1.process(hp_val)));
        let spark = self.spark_filter.process(input);

        let out = lp_val * 0.85 + spark * 0.70;
        self.env *= self.decay;
        out
    }
}

const NUM_CRACKLE_VOICES: usize = 3;

// Generates vinyl dust micro-ticks and occasional larger groove pops.
pub struct DustCrackle {
    voices: [CrackleBurst; NUM_CRACKLE_VOICES],
    sample_rate: f32,
}

impl DustCrackle {
    pub fn new(sample_rate: f32) -> Self {
        let mut dc = Self {
            voices: [CrackleBurst::new(sample_rate); NUM_CRACKLE_VOICES],
            sample_rate,
        };
        dc.reset(sample_rate);
        dc
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for voice in &mut self.voices {
            voice.reset(sample_rate);
        }
    }

    #[inline(always)]
    fn trigger_voice(&mut self, amplitude: f32) {
        let mut chosen_idx = 0;
        let mut min_env = f32::MAX;
        for (i, v) in self.voices.iter().enumerate() {
            if !v.is_active() {
                chosen_idx = i;
                break;
            }
            if v.env < min_env {
                min_env = v.env;
                chosen_idx = i;
            }
        }
        self.voices[chosen_idx].trigger(amplitude);
    }

    #[inline(always)]
    pub fn process(&mut self, rng: &mut Xorshift32, crackle_param: f32, amount: f32) -> f32 {
        let effective = (crackle_param * amount).clamp(0.0, 1.0);
        if effective <= 1e-4 {
            return 0.0;
        }

        let rate = (26.0 + effective * 200.0) * effective.sqrt();
        let trigger_prob = rate / self.sample_rate;

        let rand_val = rng.rand_uniform() * 0.5 + 0.5;
        if rand_val < trigger_prob {
            let u = rng.rand_uniform().abs();
            // Heavy-tailed distribution: mostly subtle micro-ticks,
            // medium ticks, and occasional pops around -10.5 dBFS.
            let amplitude = 0.012 + 0.048 * u.powi(3) + 0.16 * u.powi(6);
            let gain_scale = 0.45 + 0.55 * effective.sqrt();
            self.trigger_voice(amplitude * gain_scale);
        }

        let mut out = 0.0;
        for voice in &mut self.voices {
            if voice.is_active() {
                let white = rng.rand_gaussian();
                out += voice.process(white * 2.55);
            }
        }

        out
    }
}
