use super::dsp::{OnePoleHp, OnePoleLp};

// Analog vinyl warmth and pre-amp saturation.
//
// Implements gentle asymmetrical saturation (generating euphonic 2nd and 3rd harmonics)
// coupled with analog pre/post high-frequency roll-off and cartridge subsonic high-pass.
pub struct VinylWarmth {
    pre_lp: OnePoleLp,
    post_lp: OnePoleLp,
    cartridge_hp: OnePoleHp,

    sample_rate: f32,
}

impl VinylWarmth {
    pub fn new(sample_rate: f32) -> Self {
        let mut vw = Self {
            pre_lp: OnePoleLp::new(17000.0, sample_rate),
            post_lp: OnePoleLp::new(16000.0, sample_rate),
            cartridge_hp: OnePoleHp::new(38.0, sample_rate),
            sample_rate,
        };
        vw.reset(sample_rate);
        vw
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.pre_lp = OnePoleLp::new(17000.0, sample_rate);
        self.post_lp = OnePoleLp::new(16000.0, sample_rate);
        self.cartridge_hp = OnePoleHp::new(38.0, sample_rate);
    }

    #[inline(always)]
    fn velvet_saturate(x: f32, drive: f32, bias: f32) -> f32 {
        let biased = x * drive + bias;
        let bias_tanh = bias.tanh();
        let linear_gain = drive * (1.0 - bias_tanh * bias_tanh);

        let y = biased.tanh() - bias_tanh;
        y / linear_gain.max(1e-6)
    }

    #[inline(always)]
    pub fn process(&mut self, input: f32, warmth_param: f32, amount: f32) -> f32 {
        let w = (warmth_param * amount).clamp(0.0, 1.0);
        if w <= 1e-4 {
            return self.cartridge_hp.process(input);
        }

        // Dynamically adjust cutoff based on warmth
        let pre_fc = 18000.0 - (w * 5000.0);
        let post_fc = 17500.0 - (w * 4500.0);
        self.pre_lp.set_cutoff(pre_fc, self.sample_rate);
        self.post_lp.set_cutoff(post_fc, self.sample_rate);

        let drive = 1.0 + (w * 1.25);
        let bias = 0.04 * w;
        let warm_mix = 0.45 * w;

        let filtered_in = self.pre_lp.process(input);
        // Nonlinear saturation
        let saturated = Self::velvet_saturate(filtered_in, drive, bias);
        let filtered_out = self.post_lp.process(saturated);

        // Mix dry / warm and apply tonearm subsonic cut
        let mixed = input + (filtered_out - input) * warm_mix;
        self.cartridge_hp.process(mixed)
    }
}
