pub mod crackle;
pub mod dsp;
pub mod noise;
pub mod params;
pub mod warmth;
pub mod wow_flutter;

pub use params::VinylParams;

use super::Effect;
use crackle::DustCrackle;
use dsp::{Xorshift32, soft_clip};
use noise::{RumbleNoise, SurfaceNoise};
use params::VinylSmoothParams;
use warmth::VinylWarmth;
use wow_flutter::WowFlutter;

pub struct VinylEffect {
    sample_rate: f32,
    rng: Xorshift32,

    wow_flutter: WowFlutter,
    warmth: VinylWarmth,
    surface: SurfaceNoise,
    rumble: RumbleNoise,
    crackle: DustCrackle,

    smooth_params: VinylSmoothParams,
    pub params: VinylParams,
}

impl VinylEffect {
    pub fn new(sample_rate: u32, params: VinylParams, seed: u32) -> Self {
        let sr_f32 = sample_rate as f32;
        let smooth_params = VinylSmoothParams::new(&params, sr_f32);

        let mut effect = Self {
            sample_rate: sr_f32,
            rng: Xorshift32::new(seed.max(1)),

            wow_flutter: WowFlutter::new(sr_f32),
            warmth: VinylWarmth::new(sr_f32),
            surface: SurfaceNoise::new(sr_f32),
            rumble: RumbleNoise::new(sr_f32, 60.0),
            crackle: DustCrackle::new(sr_f32),

            smooth_params,
            params,
        };

        effect.reset(sr_f32);
        effect
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.wow_flutter.reset(sample_rate);
        self.warmth.reset(sample_rate);
        self.surface.reset(sample_rate);
        self.rumble.reset(sample_rate);
        self.crackle.reset(sample_rate);
        self.smooth_params.reset(sample_rate);
    }

    pub fn trigger_needle_drop(&mut self) {
        let amount = self.params.get_amount();
        let needle = self.params.get_needle();
        self.rumble.trigger_needle_drop(needle * amount);
    }

    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        // Sanitize input against NaN / Inf to protect recursive IIR filter states
        let x = if input.is_finite() { input } else { 0.0 };

        // Advance smoothed parameters per-sample to guarantee smooth transitions
        let amount = self.smooth_params.amount.next_val();
        let crackle = self.smooth_params.crackle.next_val();
        let hiss = self.smooth_params.hiss.next_val();
        let room = self.smooth_params.room.next_val();
        let wow = self.smooth_params.wow.next_val();
        let warmth_val = self.smooth_params.warmth.next_val();
        let needle = self.smooth_params.needle.next_val();
        let bypass_fade = self.smooth_params.bypass_fade.next_val();

        // Wow and Flutter pitch modulation + delay-aligned dry signal for phase-matched mixing
        let (mod_wet, dry_aligned) = self.wow_flutter.process(x, wow, amount);

        // warmth & saturation
        let warmed = self.warmth.process(mod_wet, warmth_val, amount);

        // Ducking envelope from dry audio
        let ducking = self.rumble.update_ducking(dry_aligned);

        // Noise layers
        // Surface hiss texture (multiplied noise bands calibrated to vinyl spectrum)
        let hiss_out =
            self.surface.process(&mut self.rng, self.sample_rate) * (hiss * amount * 0.85);

        // Low frequency groove friction and motor hum
        let (rumble_continuous, needle_landing) =
            self.rumble
                .process(&mut self.rng, ducking, self.sample_rate);
        let rumble_out = rumble_continuous * (room * amount * 0.22);
        let needle_out = needle_landing * (needle * amount * 0.15);

        // Dust crackle & scratches
        let crackle_out = self.crackle.process(&mut self.rng, crackle, amount * 0.75);

        let wet_combined = warmed + hiss_out + rumble_out + needle_out + crackle_out;
        let wet = soft_clip(wet_combined);

        dry_aligned + (wet - dry_aligned) * bypass_fade
    }
}

impl Effect for VinylEffect {
    #[inline]
    fn process(&mut self, sample: f32) -> f32 {
        self.process_sample(sample)
    }

    fn process_block(&mut self, data: &mut [f32]) {
        // Fast-path: if disabled and crossfade has fully decayed to 0, pass through dry
        let enabled = self.params.is_enabled();
        if !enabled && self.smooth_params.bypass_fade.current() <= 1e-5 {
            return;
        }

        // Pull latest atomic parameters to update smoothers
        self.smooth_params.update_targets(&self.params);

        for s in data.iter_mut() {
            *s = self.process_sample(*s);
        }
    }

    fn prepare(&mut self, sample_rate: u32) {
        let sr = sample_rate as f32;
        self.reset(sr);
        if self.params.is_enabled() {
            self.trigger_needle_drop();
        }
    }
}
