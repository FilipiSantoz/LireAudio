use luna_analyzer::audio;
use luna_analyzer::fx;
use luna_analyzer::metrics::{PlaybackMetrics, SinkMetrics};
use luna_analyzer::pipeline;
use luna_analyzer::routing;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fx::vinyl::VinylParams;
use fx::{FxChain, StereoFxChain, VinylEffect};
use ringbuf::{HeapRb, traits::Split};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::Manager;
use tauri::{State, ipc::Channel};

const RING_SIZE: usize = 4_096;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioDevice {
    index: usize,
    name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioDeviceList {
    inputs: Vec<AudioDevice>,
    outputs: Vec<AudioDevice>,
    preferred_input_index: Option<usize>,
    default_output_index: Option<usize>,
    routing_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioStartInfo {
    sample_rate: u32,
    input_name: String,
    output_name: Option<String>,
}

struct AppState {
    engine: Mutex<AudioEngine>,
    vinyl_params: VinylParams,
}

struct AudioEngine {
    router: Box<dyn routing::AudioRouter>,
    router_active: bool,
    router_status: String,
    input_hint: Option<String>,
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    stop_flag: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

impl AppState {
    fn new() -> Self {
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
        let mut router = routing::create_router();
        let (router_active, router_status) = match router.setup() {
            Ok(()) => (true, "Created Virtual Sink (LunaInput).".to_string()),
            Err(error) => (false, format!("Fail to Create Virtual Sink: {error}")),
        };
        let input_hint = if router_active {
            router.input_device_hint()
        } else {
            None
        };

        Self {
            router,
            router_active,
            router_status,
            input_hint,
            input_stream: None,
            output_stream: None,
            stop_flag: None,
            worker: None,
        }
    }

    fn ensure_router(&mut self) -> Result<(), String> {
        if self.router_active {
            return Ok(());
        }

        self.router
            .setup()
            .map_err(|error| format!("Fail on virtual routing: {error}"))?;

        self.router_active = true;
        self.input_hint = self.router.input_device_hint();
        self.router_status = "Virtual Routing ok (LunaInput).".to_string();
        Ok(())
    }

    fn teardown_router(&mut self) {
        if !self.router_active {
            return;
        }

        self.router.teardown();
        self.router_active = false;
        self.input_hint = None;
        self.router_status = "Stoped virtual routing.".to_string();
    }

    fn list_devices(&mut self) -> Result<AudioDeviceList, String> {
        // The router creates the virtual monitor input. If setup failed at app
        // startup, retry here but still return device lists where possible.
        if !self.router_active
            && let Err(error) = self.ensure_router()
        {
            self.router_status = error;
        }

        let host = cpal::default_host();
        let input_raw: Vec<cpal::Device> = host
            .input_devices()
            .map_err(|error| format!("Can't List Inputs: {error}"))?
            .collect();
        let output_raw: Vec<cpal::Device> = host
            .output_devices()
            .map_err(|error| format!("Can't List Outputs: {error}"))?
            .collect();

        let inputs: Vec<AudioDevice> = input_raw
            .iter()
            .enumerate()
            .map(|(index, device)| AudioDevice {
                index,
                name: device_name(device, index),
            })
            .collect();
        let outputs: Vec<AudioDevice> = output_raw
            .iter()
            .enumerate()
            .map(|(index, device)| AudioDevice {
                index,
                name: device_name(device, index),
            })
            .collect();

        let default_input_name = host
            .default_input_device()
            .as_ref()
            .map(|d| device_name(d, 0));
        let _default_output_name = host
            .default_output_device()
            .as_ref()
            .map(|d| device_name(d, 0));

        let preferred_input_index = self
            .input_hint
            .as_deref()
            .and_then(|hint| {
                let hint = hint.to_lowercase();
                inputs
                    .iter()
                    .position(|device| device.name.to_lowercase().contains(&hint))
            })
            .or_else(|| {
                default_input_name
                    .as_ref()
                    .and_then(|name| inputs.iter().position(|device| device.name == *name))
            })
            .or_else(|| (!inputs.is_empty()).then_some(0));

        // let default_output_index = default_output_name
        //     .as_ref()
        //     .and_then(|name| outputs.iter().position(|device| device.name == *name))
        //     .or_else(|| (!outputs.is_empty()).then_some(0));

        Ok(AudioDeviceList {
            inputs,
            outputs,
            preferred_input_index,
            default_output_index: None,
            routing_status: self.router_status.clone(),
        })
    }

    fn start(
        &mut self,
        input_index: usize,
        output_index: Option<usize>,
        vinyl_params: VinylParams,
        spectrum_channel: Channel<Vec<f32>>,
    ) -> Result<AudioStartInfo, String> {
        self.stop_streams();
        self.ensure_router()?;

        let host = cpal::default_host();
        let input_devices: Vec<cpal::Device> = host
            .input_devices()
            .map_err(|error| format!("Fail to List Input Devices: {error}"))?
            .collect();
        let input_device = input_devices
            .into_iter()
            .nth(input_index)
            .ok_or_else(|| format!("Invalid Entry index: {input_index}"))?;

        let input_name = device_name(&input_device, input_index);
        let input_config = input_device
            .default_input_config()
            .map_err(|error| format!("Failt to config input: {error}"))?;
        let sample_rate = input_config.sample_rate();

        let input_rb = HeapRb::<f32>::new(RING_SIZE);
        let (input_producer, input_consumer) = input_rb.split();
        let input_stream =
            audio::build_input_stream_for_device(&input_device, input_config, input_producer)
                .map_err(|error| format!("Fail to create Input Stream: {error}"))?;

        let mut output_stream: Option<cpal::Stream> = None;
        let mut output_producer: Option<ringbuf::HeapProd<f32>> = None;
        let mut output_name: Option<String> = None;

        if let Some(index) = output_index {
            let output_devices: Vec<cpal::Device> = host
                .output_devices()
                .map_err(|error| format!("Fail to list Output Devices: {error}"))?
                .collect();
            let output_device = output_devices
                .into_iter()
                .nth(index)
                .ok_or_else(|| format!("Invalid Output Index: {index}"))?;

            output_name = Some(device_name(&output_device, index));

            let ranges = output_device
                .supported_output_configs()
                .map_err(|error| format!("Fail to consult the Output Device: {error}"))?;
            let output_config = ranges
                .filter(|range| {
                    matches!(
                        range.sample_format(),
                        cpal::SampleFormat::F32
                            | cpal::SampleFormat::F64
                            | cpal::SampleFormat::I8
                            | cpal::SampleFormat::I16
                            | cpal::SampleFormat::I24
                            | cpal::SampleFormat::I32
                            | cpal::SampleFormat::I64
                    )
                })
                .find_map(|range| range.try_with_sample_rate(sample_rate))
                .ok_or_else(|| {
                    format!(
                        "The selected Output Device does not support {} hz. Select other Output Device; resampling not implemented yet.",
                        sample_rate
                    )
                })?;

            let output_rb = HeapRb::<f32>::new(RING_SIZE * 2);
            let (producer, consumer) = output_rb.split();
            output_stream = Some(
                audio::build_output_stream_for_device(&output_device, output_config, consumer)
                    .map_err(|error| format!("Fail to create Output Stream: {error}"))?,
            );
            output_producer = Some(producer);
        }

        let mut left = FxChain::new();
        left.add(VinylEffect::new(
            sample_rate,
            vinyl_params.clone(),
            0x9E3779B9,
        ));

        let mut right = FxChain::new();
        right.add(VinylEffect::new(sample_rate, vinyl_params, 0x12345678));

        // Start the streams only after all configurations are validated.
        input_stream
            .play()
            .map_err(|error| format!("Fail to Init Input Device: {error}"))?;
        if let Some(stream) = output_stream.as_ref() {
            stream
                .play()
                .map_err(|error| format!("Fail to init Output Device: {error}"))?;
        }

        let stereo_fx = StereoFxChain::new(left, right);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let sink_metrics = Arc::new(SinkMetrics::default());
        let playback_metrics = Arc::new(PlaybackMetrics::default());
        let worker = thread::Builder::new()
            .name("luna-audio-pipeline".to_string())
            .spawn(move || {
                pipeline::run_audio_processor(
                    input_consumer,
                    output_producer,
                    sample_rate,
                    stereo_fx,
                    worker_stop,
                    move |bands| {
                        if let Err(e) = spectrum_channel.send(bands.to_vec()) {
                            eprintln!("Warn: Fail so send bands via Channel: {e}");
                        }
                    },
                    Arc::clone(&sink_metrics),
                    Arc::clone(&playback_metrics),
                );
            })
            .map_err(|error| format!("Fail to init audio thread: {error}"))?;

        self.input_stream = Some(input_stream);
        self.output_stream = output_stream;
        self.stop_flag = Some(stop);
        self.worker = Some(worker);

        Ok(AudioStartInfo {
            sample_rate,
            input_name,
            output_name,
        })
    }

    fn stop_streams(&mut self) {
        if let Some(stop) = self.stop_flag.take() {
            stop.store(true, Ordering::Relaxed);
        }

        self.input_stream.take();
        self.output_stream.take();

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop_streams();
    }
}

fn device_name(device: &cpal::Device, index: usize) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| format!("Audio device {index}"))
}

#[tauri::command]
fn list_audio_devices(state: State<'_, AppState>) -> Result<AudioDeviceList, String> {
    state
        .engine
        .lock()
        .map_err(|_| "Audio State Unavailible.".to_string())?
        .list_devices()
}

#[tauri::command]
fn start_audio(
    state: State<'_, AppState>,
    input_index: usize,
    output_index: Option<usize>,
    spectrum_channel: Channel<Vec<f32>>,
) -> Result<AudioStartInfo, String> {
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| "Audio State Unavailible.".to_string())?;

    let result = engine.start(
        input_index,
        output_index,
        state.vinyl_params.clone(),
        spectrum_channel,
    );

    if result.is_err() {
        engine.stop_streams();
        engine.teardown_router();
    }

    result
}

#[tauri::command]
fn stop_audio(state: State<'_, AppState>) -> Result<(), String> {
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| "Audio State Unavailable.".to_string())?;
    engine.stop_streams();
    engine.teardown_router();
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
            list_audio_devices,
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
            engine.stop_streams();
            engine.teardown_router();
        }
    });
}
