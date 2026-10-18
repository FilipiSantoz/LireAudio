use luna_analyzer::{metrics::AudioMetrics, pipewire::PipewireEngine};
use ringbuf::traits::Split;
use std::time::Duration;

const RING_SIZE: usize = 16384;

#[test]
fn test_pipewire_lifecycle() {
    let mut engine = PipewireEngine::new();
    let rb = ringbuf::HeapRb::<f32>::new(RING_SIZE);
    let (producer, _consumer) = rb.split();
    let metrics = std::sync::Arc::new(AudioMetrics::default());

    let start = engine.start(None, producer, metrics);
    assert!(start.is_ok(), "Failt to connect: {:?}", start.is_err());

    std::thread::sleep(Duration::from_secs(1));

    engine.stop();
}
