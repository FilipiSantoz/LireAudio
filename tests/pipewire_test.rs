use luna_analyzer::pipewire::PipewireEngine;
use ringbuf::{HeapRb, traits::Split};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

const RING_SIZE: usize = 16384;

#[test]
fn test_pipewire_lifecycle() {
    let rb_in = HeapRb::new(RING_SIZE);
    let (in_producer, _in_consumer) = rb_in.split();

    let rb_out = HeapRb::new(RING_SIZE * 2);
    let (_out_producer, out_consumer) = rb_out.split();

    let mut engine = PipewireEngine::new();
    let chunk_hint = Arc::new(AtomicUsize::new(0));
    engine
        .start(in_producer, Some(out_consumer), Arc::clone(&chunk_hint))
        .expect("Fail to create Pipewire Node");

    std::thread::sleep(Duration::from_secs(1));

    engine.stop();
}
