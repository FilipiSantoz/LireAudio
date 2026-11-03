use lire_audio::audio_capture::{
    INPUT_CAPACITY, PIPEWIRE_SAMPLE_RATE, PipewireEngine, create_analysis_pipe,
};
use lire_audio::fx::{FxChain, StereoFxChain};
use lire_audio::pipeline::{self, AudioProcessor, new_frame_queue};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

fn main() {
    let stop = Arc::new(AtomicBool::new(false));

    let stop_for_stdin = Arc::clone(&stop);
    thread::spawn(move || {
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).is_ok() && line.trim().eq_ignore_ascii_case("q") {
            stop_for_stdin.store(true, Ordering::Relaxed);
        }
    });

    let chunk_hint = Arc::new(AtomicUsize::new(0));
    let (tap, input) = create_analysis_pipe(INPUT_CAPACITY);
    let stereo_fx = StereoFxChain::new(FxChain::new(), FxChain::new());
    let mut engine = PipewireEngine::new();
    if let Err(e) = engine.start(tap, stereo_fx, Arc::clone(&chunk_hint)) {
        eprintln!("Can't init Pipewire engine: {e}");
        return;
    }

    let frames = new_frame_queue();
    let stop_for_render = Arc::clone(&stop);

    let processor = AudioProcessor {
        input,
        sample_rate: PIPEWIRE_SAMPLE_RATE,
        stop: Arc::clone(&stop),
        frames: Arc::clone(&frames),
        chunk_hint,
    };

    thread::spawn(move || processor.run());
    println!("Listening...");

    pipeline::terminal_run(frames, stop_for_render);

    stop.store(true, Ordering::Relaxed);
    engine.stop();
    println!("Bye");
}
