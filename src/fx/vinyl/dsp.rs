use std::f32::consts::PI;

// Fast, deterministic 32-bit PRNG for audio noise generation.
#[derive(Clone, Copy)]
pub struct Xorshift32 {
    state: u32,
}

impl Xorshift32 {
    #[inline(always)]
    pub fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline(always)]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    // Uniform float in `[-1.0, 1.0]`.
    #[inline(always)]
    pub fn rand_uniform(&mut self) -> f32 {
        let u = self.next_u32();
        (u as f32 / (u32::MAX as f32 + 1.0)) * 2.0 - 1.0
    }

    // Approximate Gaussian noise via Irwin-Hall distribution (sum of 3 uniforms).
    #[inline(always)]
    pub fn rand_gaussian(&mut self) -> f32 {
        let mut sum = 0.0;
        for _ in 0..3 {
            sum += self.rand_uniform();
        }
        sum * 0.577_350_26
    }
}

// 1-pole Low-Pass filter: y[n] = y[n-1] + alpha * (x[n] - y[n-1])
#[derive(Default, Clone, Copy)]
pub struct OnePoleLp {
    alpha: f32,
    state: f32,
}

impl OnePoleLp {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let mut lp = Self::default();
        lp.set_cutoff(cutoff_hz, sample_rate);
        lp
    }

    #[inline(always)]
    pub fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        if sample_rate <= 0.0 {
            return;
        }
        let fc = cutoff_hz.clamp(1.0, sample_rate * 0.499);
        self.alpha = (1.0 - (-2.0 * PI * fc / sample_rate).exp()).clamp(0.0, 1.0);
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        self.state += self.alpha * (x - self.state);
        self.state
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.state = 0.0;
    }
}

// 1-pole High-Pass filter: y[n] = alpha * (y[n-1] + x[n] - x[n-1])
#[derive(Default, Clone, Copy)]
pub struct OnePoleHp {
    alpha: f32,
    state: f32,
    last_in: f32,
}

impl OnePoleHp {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let mut hp = Self::default();
        hp.set_cutoff(cutoff_hz, sample_rate);
        hp
    }

    #[inline(always)]
    pub fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        if sample_rate <= 0.0 {
            return;
        }
        let fc = cutoff_hz.clamp(0.1, sample_rate * 0.499);
        let dt = 1.0 / sample_rate;
        let rc = 1.0 / (2.0 * PI * fc);
        self.alpha = (rc / (rc + dt)).clamp(0.0, 1.0);
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.alpha * (self.state + x - self.last_in);
        self.state = y;
        self.last_in = x;
        y
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.state = 0.0;
        self.last_in = 0.0;
    }
}

// DC Blocker: high-pass filter at very low frequency to remove constant bias.
#[derive(Default, Clone, Copy)]
pub struct DcBlocker {
    r: f32,
    x_prev: f32,
    y_prev: f32,
}

impl DcBlocker {
    pub fn new(sample_rate: f32) -> Self {
        let mut blocker = Self::default();
        blocker.set_sample_rate(sample_rate);
        blocker
    }

    #[inline(always)]
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.r = 1.0 - (2.0 * PI * 15.0 / sample_rate);
        }
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x_prev + self.r * self.y_prev;
        self.x_prev = x;
        self.y_prev = y;
        y
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.x_prev = 0.0;
        self.y_prev = 0.0;
    }
}

// Biquad filter (Direct Form II Transposed) for resonant bandpass, rumble, or notch filtering.
#[derive(Default, Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn bandpass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let mut bq = Self::default();
        bq.set_bandpass(freq, q, sample_rate);
        bq
    }

    #[inline(always)]
    pub fn set_bandpass(&mut self, freq: f32, q: f32, sample_rate: f32) {
        if sample_rate <= 0.0 {
            return;
        }
        let w0 = 2.0 * PI * (freq / sample_rate);
        let alpha = w0.sin() / (2.0 * q.max(0.01));
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;

        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let out = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * out + self.z2;
        self.z2 = self.b2 * x - self.a2 * out;
        out
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

// Parameter smoother (1-pole lowpass) to eliminate zipper noise when sliders move.
#[derive(Clone, Copy)]
pub struct Smoothed {
    current: f32,
    target: f32,
    alpha: f32,
}

impl Smoothed {
    pub fn new(initial: f32, time_constant_sec: f32, sample_rate: f32) -> Self {
        let alpha = if sample_rate > 0.0 && time_constant_sec > 0.0 {
            1.0 - (-1.0 / (time_constant_sec * sample_rate)).exp()
        } else {
            1.0
        };
        Self {
            current: initial,
            target: initial,
            alpha,
        }
    }

    #[inline(always)]
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    #[inline(always)]
    pub fn next_val(&mut self) -> f32 {
        self.current += self.alpha * (self.target - self.current);
        self.current
    }

    #[inline(always)]
    pub fn current(&self) -> f32 {
        self.current
    }

    #[inline(always)]
    pub fn snap_to_target(&mut self) {
        self.current = self.target;
    }
}

// 4-point Hermite interpolation for fractional delay line sampling.
#[inline(always)]
pub fn hermite_interp(y0: f32, y1: f32, y2: f32, y3: f32, mu: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * mu + c2) * mu + c1) * mu + c0
}

// Soft clipping / saturation to protect against harsh distortion and maintain analog headroom.
#[inline(always)]
pub fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.75 {
        x
    } else if x > 0.0 {
        1.0 - (-x + 0.75).exp() * 0.25
    } else {
        -1.0 + (x + 0.75).exp() * 0.25
    }
}
