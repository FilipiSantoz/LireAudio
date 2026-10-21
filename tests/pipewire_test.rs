use luna_analyzer::audio_capture::{INPUT_CAPACITY, OUTPUT_CAPACITY, PipewireEngine, create_pipes};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[test]
fn test_pipewire_lifecycle() {
    let (capture, _process, playback) = create_pipes(INPUT_CAPACITY, OUTPUT_CAPACITY);

    let mut engine = PipewireEngine::new();
    let chunk_hint = Arc::new(AtomicUsize::new(0));
    engine
        .start(capture, Some(playback), Arc::clone(&chunk_hint))
        .expect("Fail to create Pipewire Node");

    std::thread::sleep(Duration::from_secs(1));

    engine.stop();
}
