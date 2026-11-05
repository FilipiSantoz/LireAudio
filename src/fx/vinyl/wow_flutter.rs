use super::dsp::hermite_interp;
use std::f32::consts::PI;

const DELAY_CAPACITY: usize = 4096;
const DELAY_MASK: usize = DELAY_CAPACITY - 1;

// Vinyl pitch modulation (Wow and Flutter).
//
// Models turntable motor speed variations:
// - Wow: low frequency variations at 0.5555 Hz (corresponding to 33⅓ RPM) + subtle slow drift.
// - Flutter: higher frequency mechanical flutter at 2.6 Hz and 5.5 Hz.
pub struct WowFlutter {
    buffer: Vec<f32>,
    write_pos: usize,

    phase_wow: f32,
    phase_drift: f32,
    phase_flutter1: f32,
    phase_flutter2: f32,

    sample_rate: f32,
}

impl WowFlutter {
    pub fn new(sample_rate: f32) -> Self {
        let mut wf = Self {
            buffer: vec![0.0; DELAY_CAPACITY],
            write_pos: 0,
            phase_wow: 0.0,
            phase_drift: 0.0,
            phase_flutter1: 0.0,
            phase_flutter2: 0.0,
            sample_rate,
        };
        wf.reset(sample_rate);
        wf
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.phase_wow = 0.0;
        self.phase_drift = 0.0;
        self.phase_flutter1 = 0.0;
        self.phase_flutter2 = 0.0;
        self.sample_rate = sample_rate;
    }

    // - `modulated_wet`: pitch-modulated signal.
    // - `aligned_dry`: dry signal delayed by the nominal center delay (3 ms) to allow
    //   seamless phase-aligned dry/wet crossfading without comb filtering.
    #[inline(always)]
    pub fn process(&mut self, input: f32, wow_param: f32, amount: f32) -> (f32, f32) {
        self.buffer[self.write_pos] = input;

        let sr = self.sample_rate;
        let depth = (wow_param * amount).clamp(0.0, 1.0);

        // Advance phases
        self.phase_wow = (self.phase_wow + 0.5555 / sr).rem_euclid(1.0);
        self.phase_drift = (self.phase_drift + 0.12 / sr).rem_euclid(1.0);
        self.phase_flutter1 = (self.phase_flutter1 + 2.6 / sr).rem_euclid(1.0);
        self.phase_flutter2 = (self.phase_flutter2 + 5.7 / sr).rem_euclid(1.0);

        // Calculate wow & flutter modulation in seconds
        // Wow ~ 0.08% to 0.15% speed variation at full depth
        let wow_sine = (self.phase_wow * 2.0 * PI).sin();
        let drift_sine = (self.phase_drift * 2.0 * PI).sin() * 0.25;
        let wow_s = (wow_sine + drift_sine) * depth * 0.0012;

        let flutter_s = ((self.phase_flutter1 * 2.0 * PI).sin() * 0.7
            + (self.phase_flutter2 * 2.0 * PI).sin() * 0.3)
            * depth
            * 0.0006;

        let center_delay_sec = 0.0035; // 3.5 ms center delay
        let delay_samples = (center_delay_sec + wow_s + flutter_s) * sr;
        let nominal_delay_samples = center_delay_sec * sr;

        let len_f = DELAY_CAPACITY as f32;

        // Modulated read position
        let read_pos = (self.write_pos as f32 - delay_samples).rem_euclid(len_f);
        let i0 = read_pos.floor() as isize;
        let frac = read_pos - i0 as f32;
        let mask = DELAY_MASK as isize;

        let y0 = self.buffer[((i0 - 1) & mask) as usize];
        let y1 = self.buffer[(i0 & mask) as usize];
        let y2 = self.buffer[((i0 + 1) & mask) as usize];
        let y3 = self.buffer[((i0 + 2) & mask) as usize];
        let modulated_wet = hermite_interp(y0, y1, y2, y3, frac);

        // Aligned dry read position (nominal center delay)
        let dry_pos = (self.write_pos as f32 - nominal_delay_samples).rem_euclid(len_f);
        let d0 = dry_pos.floor() as isize;
        let d_frac = dry_pos - d0 as f32;

        let dy0 = self.buffer[((d0 - 1) & mask) as usize];
        let dy1 = self.buffer[(d0 & mask) as usize];
        let dy2 = self.buffer[((d0 + 1) & mask) as usize];
        let dy3 = self.buffer[((d0 + 2) & mask) as usize];
        let aligned_dry = hermite_interp(dy0, dy1, dy2, dy3, d_frac);

        self.write_pos = (self.write_pos + 1) & DELAY_MASK;

        (modulated_wet, aligned_dry)
    }
}
