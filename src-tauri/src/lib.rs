use luna_analyzer::audio_capture::{
    INPUT_CAPACITY, OUTPUT_CAPACITY, PIPEWIRE_SAMPLE_RATE, PipewireEngine, create_pipes,
};
use luna_analyzer::fx::vinyl::VinylParams;
use luna_analyzer::fx::{FxChain, StereoFxChain, VinylEffect};
use luna_analyzer::pipeline::{self, new_frame_queue, next_present_at, take_due_frame};

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri::{State, ipc::Channel};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioState {
    Running,
    Stopped,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioStartInfo {
    sample_rate: f32,
    status: AudioState,
}

struct AudioEngine {
    pw_engine: PipewireEngine,
    pipeline_worker: Option<JoinHandle<()>>,
    emitter_worker: Option<JoinHandle<()>>,
    stop_flag: Option<Arc<AtomicBool>>,
}

struct AppState {
    engine: Mutex<AudioEngine>,
    vinyl_params: VinylParams,
}

impl AppState {
    pub fn new() -> Self {
        let vinyl_params = VinylParams::default();
        vinyl_params.enabled.store(false, Ordering::Relaxed);
        Self {
            engine: Mutex::new(AudioEngine::new()),
            vinyl_params,
        }
    }
}

impl AudioEngine {
    fn new() -> Self {
        let engine = PipewireEngine::new();

        Self {
            pw_engine: engine,
            pipeline_worker: None,
            emitter_worker: None,
            stop_flag: None,
        }
    }

    fn stop(&mut self) {
        if let Some(stop) = self.stop_flag.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.pw_engine.stop();
        if let Some(handle) = self.pipeline_worker.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.emitter_worker.take() {
            let _ = handle.join();
        }
    }

    fn start(
        &mut self,
        vinyl_params: VinylParams,
        spectrum_channel: Channel<Vec<f32>>,
    ) -> Result<AudioStartInfo, String> {
        self.stop();

        let stop = Arc::new(AtomicBool::new(false));
        let frames = new_frame_queue();
        let chunk_hint = Arc::new(AtomicUsize::new(0));

        let (capture, process, playback) = create_pipes(INPUT_CAPACITY, OUTPUT_CAPACITY);

        let mut engine = PipewireEngine::new();
        engine
            .start(capture, Some(playback), Arc::clone(&chunk_hint))
            .expect("Fail to init Pipewire");

        let mut left = FxChain::new();
        left.add(VinylEffect::new(
            PIPEWIRE_SAMPLE_RATE,
            vinyl_params.clone(),
            0x9E3779B9,
        ));

        let mut right = FxChain::new();
        right.add(VinylEffect::new(
            PIPEWIRE_SAMPLE_RATE,
            vinyl_params,
            0x12345678,
        ));
        let stereo_fx = StereoFxChain::new(left, right);
        let stop_for_worker = Arc::clone(&stop);
        let frames_for_worker = Arc::clone(&frames);
        let hint_for_worker = Arc::clone(&chunk_hint);

        let pipeline_worker = thread::Builder::new()
            .name("luna-analysis".into())
            .spawn(move || {
                pipeline::run_audio_processor(
                    process,
                    PIPEWIRE_SAMPLE_RATE,
                    stereo_fx,
                    stop_for_worker,
                    frames_for_worker,
                    hint_for_worker,
                );
            })
            .map_err(|e| format!("Fail to init Pipeline Thread: {e}"))?;

        let stop_for_emitter = Arc::clone(&stop);
        let frames_for_emitter = Arc::clone(&frames);

        let emitter_worker = thread::Builder::new()
            .name("luna-tauri-emitter".into())
            .spawn(move || {
                while !stop_for_emitter.load(Ordering::Relaxed) {
                    if let Some(bands) = take_due_frame(&frames_for_emitter, Instant::now())
                        && spectrum_channel.send(bands.to_vec()).is_err()
                    {
                        break;
                    }
                    let wait = next_present_at(&frames_for_emitter)
                        .map(|t| t.saturating_duration_since(Instant::now()))
                        .unwrap_or(Duration::from_millis(8))
                        .min(Duration::from_millis(8))
                        .max(Duration::from_millis(1));
                    thread::sleep(wait);
                }
            })
            .map_err(|e| format!("Fail to init Emitter Thread: {e}"))?;

        self.pw_engine = engine;
        self.pipeline_worker = Some(pipeline_worker);
        self.emitter_worker = Some(emitter_worker);
        self.stop_flag = Some(stop);

        Ok(AudioStartInfo {
            sample_rate: PIPEWIRE_SAMPLE_RATE as f32,
            status: AudioState::Running,
        })
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[tauri::command]
fn start_audio(
    state: State<'_, AppState>,
    spectrum_channel: Channel<Vec<f32>>,
) -> Result<AudioStartInfo, String> {
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| "Audio State Unavailible.".to_string())?;

    engine.start(state.vinyl_params.clone(), spectrum_channel)
}

#[tauri::command]
fn stop_audio(state: State<'_, AppState>) -> Result<(), String> {
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| "Audio State Unavailable.".to_string())?;
    engine.stop();
    Ok(())
}

#[tauri::command]
fn set_vinyl_enabled(state: State<'_, AppState>, enabled: bool) {
    state.vinyl_params.enabled.store(enabled, Ordering::Relaxed);
}

#[tauri::command]
fn set_vinyl_parameter(state: State<'_, AppState>, name: String, value: f32) -> Result<(), String> {
    if !value.is_finite() {
        return Err("Need a finite number for the parameter.".to_string());
    }
    let value = value.clamp(0.0, 1.0).to_bits();
    let target = match name.as_str() {
        "amount" => &state.vinyl_params.amount,
        "crackle" => &state.vinyl_params.crackle,
        "hiss" => &state.vinyl_params.hiss,
        "room" => &state.vinyl_params.room,
        "wow" => &state.vinyl_params.wow,
        "warmth" => &state.vinyl_params.warmth,
        "needle" => &state.vinyl_params.needle,
        other => return Err(format!("Unknown vinyl parameter: {other}")),
    };
    target.store(value, Ordering::Relaxed);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_audio,
            stop_audio,
            set_vinyl_enabled,
            set_vinyl_parameter
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri config");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event
            && let Some(state) = app_handle.try_state::<AppState>()
            && let Ok(mut engine) = state.engine.lock()
        {
            println!("Closing App");
            engine.stop();
        }
    });
}
