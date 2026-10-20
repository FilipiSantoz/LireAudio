use luna_analyzer::pipewire::PipewireEngine;
use ringbuf::{HeapRb, traits::Split};
use std::time::Duration;

const RING_SIZE: usize = 16384;

#[test]
fn test_pipewire_lifecycle() {
    let rb_in = HeapRb::new(RING_SIZE);
    let (in_producer, _in_consumer) = rb_in.split();

    let rb_out = HeapRb::new(RING_SIZE * 2);
    let (_out_producer, out_consumer) = rb_out.split();

    let mut engine = PipewireEngine::new();
    engine
        .start(in_producer, Some(out_consumer))
        .expect("Fail to create Pipewire Node");

    std::thread::sleep(Duration::from_secs(1));

    engine.stop();
}
