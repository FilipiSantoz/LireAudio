use luna_analyzer::pipewire::PipewireEngine;
use std::time::Duration;

#[test]
fn test_pipewire_lifecycle() {
    let mut engine = PipewireEngine::new();

    let start = engine.start();

    assert!(
        start.is_ok(),
        "Falha ao conectar no socket Pipewire: {:?}",
        start.err()
    );

    std::thread::sleep(Duration::from_secs(2));

    engine.stop();
}
