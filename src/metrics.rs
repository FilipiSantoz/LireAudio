use std::sync::atomic::{AtomicU64, AtomicUsize};

#[derive(Default)]
pub struct AudioMetrics {
    pub capture_callbacks: AtomicU64,
    pub capture_frames: AtomicU64,
    pub enqueued_frames: AtomicU64,
    pub dropped_frames: AtomicU64,
    pub last_callback_frames: AtomicUsize,
    pub max_callback_frames: AtomicUsize,
}

pub fn update_max_usize(target: &AtomicUsize, value: usize) {
    target.fetch_max(value, std::sync::atomic::Ordering::Relaxed);
}
