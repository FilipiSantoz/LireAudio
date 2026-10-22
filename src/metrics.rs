use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

pub static CAPTURE_CB: AtomicU64 = AtomicU64::new(0);
pub static FRAMES_IN: AtomicU64 = AtomicU64::new(0);
pub static FFT_RUNS: AtomicU64 = AtomicU64::new(0);
pub static QUEUE_DROPS: AtomicU64 = AtomicU64::new(0);
pub static QUEUE_EMPTY_AT_TAKE: AtomicU64 = AtomicU64::new(0);
pub static LATENESS_MAX_US: AtomicU64 = AtomicU64::new(0);
pub static SENT_MSG: AtomicU64 = AtomicU64::new(0);
pub static SUPERSEDED: AtomicU64 = AtomicU64::new(0);
pub static OUT_SHORT: AtomicU64 = AtomicU64::new(0);
pub static UNDERRUNS: AtomicU64 = AtomicU64::new(0);
pub static IDLE_CYCLES: AtomicU64 = AtomicU64::new(0);
// pub static SILENCE_BLOCKS_PROCESSED: AtomicU64 = AtomicU64::new(0);
pub static IDLE_RESETS: AtomicU64 = AtomicU64::new(0);

pub fn report() {
    let cb = CAPTURE_CB.swap(0, Relaxed);
    let fr = FRAMES_IN.swap(0, Relaxed);
    let fft = FFT_RUNS.swap(0, Relaxed);
    let drop = QUEUE_DROPS.swap(0, Relaxed);
    let empty = QUEUE_EMPTY_AT_TAKE.swap(0, Relaxed);
    let late = LATENESS_MAX_US.swap(0, Relaxed);
    let sent = SENT_MSG.swap(0, Relaxed);
    let seded = SUPERSEDED.swap(0, Relaxed);
    let short = OUT_SHORT.swap(0, Relaxed);
    let under = UNDERRUNS.swap(0, Relaxed);
    let idle = IDLE_CYCLES.swap(0, Relaxed);
    // let silence = SILENCE_BLOCKS_PROCESSED.swap(0, Relaxed);
    let resets = IDLE_RESETS.swap(0, Relaxed);

    println!(
        "[Backend Metrics] cb={cb}/s fr={fr}/s fft={fft}/s sent={sent}/s  seded={seded}/s \n| \
         q_drop={drop} q_empty={empty}  \n| \
         idle={idle}  resets={resets}  \n| \
         late_max={:.1}ms out_short={short} underrun={under} \n",
        late as f64 / 1000.0
    );
}
