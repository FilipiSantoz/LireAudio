use std::sync::atomic::{AtomicU64, AtomicUsize};

#[derive(Default)]
pub struct SinkMetrics {
    pub sink_callbacks: AtomicU64,
    pub sink_frames: AtomicU64,
    pub enqueued_frames: AtomicU64,
    pub dropped_frames: AtomicU64,
    pub last_callback_frames: AtomicUsize,
    pub max_callback_frames: AtomicUsize,
}

#[derive(Default)]
pub struct PlaybackMetrics {
    pub playback_callbacks: AtomicU64,
    pub requested_frames: AtomicU64,
    pub written_frames: AtomicU64,
    pub underrun_frames: AtomicU64,
    pub no_request_callbacks: AtomicU64,
    pub no_buffer_callbacks: AtomicU64,
}

pub fn update_max_usize(target: &AtomicUsize, value: usize) {
    target.fetch_max(value, std::sync::atomic::Ordering::Relaxed);
}
