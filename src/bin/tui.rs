use lire_audio::audio_capture::{
    INPUT_CAPACITY, OUTPUT_CAPACITY, PIPEWIRE_SAMPLE_RATE, PipewireEngine, create_pipes,
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
    let (capture_end, process_end, playback_end) = create_pipes(INPUT_CAPACITY, OUTPUT_CAPACITY);
    let mut engine = PipewireEngine::new();
    if let Err(e) = engine.start(capture_end, Some(playback_end), Arc::clone(&chunk_hint)) {
        eprintln!("Can't init Pipewire engine: {e}");
        return;
    }

    let frames = new_frame_queue();
    let stop_for_render = Arc::clone(&stop);
    let stereo_fx = StereoFxChain::new(FxChain::new(), FxChain::new());

    let processor = AudioProcessor {
        process: process_end,
        sample_rate: PIPEWIRE_SAMPLE_RATE,
        stereo_fx,
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
