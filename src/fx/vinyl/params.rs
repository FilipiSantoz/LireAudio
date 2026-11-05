use super::dsp::Smoothed;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// Thread-safe parameters for the Vinyl Effect.
//
// Fields use `AtomicU32` holding `f32.to_bits()` to allow lock-free real-time updates
// from GUI or Tauri threads without audio thread contention.
#[derive(Clone)]
pub struct VinylParams {
    pub enabled: Arc<AtomicBool>,
    pub amount: Arc<AtomicU32>,
    pub crackle: Arc<AtomicU32>,
    pub hiss: Arc<AtomicU32>,
    pub room: Arc<AtomicU32>,
    pub wow: Arc<AtomicU32>,
    pub warmth: Arc<AtomicU32>,
    pub needle: Arc<AtomicU32>,
}

impl Default for VinylParams {
    fn default() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            amount: Arc::new(AtomicU32::new(0.50f32.to_bits())),
            crackle: Arc::new(AtomicU32::new(0.35f32.to_bits())),
            hiss: Arc::new(AtomicU32::new(0.30f32.to_bits())),
            room: Arc::new(AtomicU32::new(0.25f32.to_bits())),
            wow: Arc::new(AtomicU32::new(0.25f32.to_bits())),
            warmth: Arc::new(AtomicU32::new(0.40f32.to_bits())),
            needle: Arc::new(AtomicU32::new(0.20f32.to_bits())),
        }
    }
}

impl VinylParams {
    #[inline(always)]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn set_enabled(&self, val: bool) {
        self.enabled.store(val, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_amount(&self) -> f32 {
        f32::from_bits(self.amount.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_amount(&self, val: f32) {
        self.amount
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_crackle(&self) -> f32 {
        f32::from_bits(self.crackle.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_crackle(&self, val: f32) {
        self.crackle
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_hiss(&self) -> f32 {
        f32::from_bits(self.hiss.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_hiss(&self, val: f32) {
        self.hiss
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_room(&self) -> f32 {
        f32::from_bits(self.room.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_room(&self, val: f32) {
        self.room
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_wow(&self) -> f32 {
        f32::from_bits(self.wow.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_wow(&self, val: f32) {
        self.wow
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_warmth(&self) -> f32 {
        f32::from_bits(self.warmth.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_warmth(&self, val: f32) {
        self.warmth
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_needle(&self) -> f32 {
        f32::from_bits(self.needle.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_needle(&self, val: f32) {
        self.needle
            .store(val.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

// Internal smoothed parameters running on the audio thread.
//
// Prevents zipper noise and clicks when sliders are adjusted by ramping
// smoothly over ~30 milliseconds.
pub struct VinylSmoothParams {
    pub amount: Smoothed,
    pub crackle: Smoothed,
    pub hiss: Smoothed,
    pub room: Smoothed,
    pub wow: Smoothed,
    pub warmth: Smoothed,
    pub needle: Smoothed,
    pub bypass_fade: Smoothed,
}

impl VinylSmoothParams {
    pub fn new(params: &VinylParams, sample_rate: f32) -> Self {
        let tau = 0.035; // 35 ms smoothing time constant
        let enabled = if params.is_enabled() { 1.0 } else { 0.0 };
        Self {
            amount: Smoothed::new(params.get_amount(), tau, sample_rate),
            crackle: Smoothed::new(params.get_crackle(), tau, sample_rate),
            hiss: Smoothed::new(params.get_hiss(), tau, sample_rate),
            room: Smoothed::new(params.get_room(), tau, sample_rate),
            wow: Smoothed::new(params.get_wow(), tau, sample_rate),
            warmth: Smoothed::new(params.get_warmth(), tau, sample_rate),
            needle: Smoothed::new(params.get_needle(), tau, sample_rate),
            bypass_fade: Smoothed::new(enabled, 0.020, sample_rate), // 20 ms crossfade on bypass
        }
    }

    pub fn update_targets(&mut self, params: &VinylParams) {
        self.amount.set_target(params.get_amount());
        self.crackle.set_target(params.get_crackle());
        self.hiss.set_target(params.get_hiss());
        self.room.set_target(params.get_room());
        self.wow.set_target(params.get_wow());
        self.warmth.set_target(params.get_warmth());
        self.needle.set_target(params.get_needle());
        let enabled = if params.is_enabled() { 1.0 } else { 0.0 };
        self.bypass_fade.set_target(enabled);
    }

    pub fn reset(&mut self, sample_rate: f32) {
        let tau = 0.035;
        self.amount = Smoothed::new(self.amount.current(), tau, sample_rate);
        self.crackle = Smoothed::new(self.crackle.current(), tau, sample_rate);
        self.hiss = Smoothed::new(self.hiss.current(), tau, sample_rate);
        self.room = Smoothed::new(self.room.current(), tau, sample_rate);
        self.wow = Smoothed::new(self.wow.current(), tau, sample_rate);
        self.warmth = Smoothed::new(self.warmth.current(), tau, sample_rate);
        self.needle = Smoothed::new(self.needle.current(), tau, sample_rate);
        self.bypass_fade = Smoothed::new(self.bypass_fade.current(), 0.020, sample_rate);
    }
}
