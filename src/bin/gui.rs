// use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui::{self};
// use luna_analyzer::audio::{self};
// use luna_analyzer::fx::vinyl::VinylParams;
use luna_analyzer::fx::{FxChain, StereoFxChain};
use luna_analyzer::metrics::{PlaybackMetrics, SinkMetrics};
use luna_analyzer::pipeline::{self, NUM_BANDS};
use luna_analyzer::pipewire::{PIPEWIRE_SAMPLE_RATE, PipewireEngine};
// use luna_analyzer::routing::{AudioRouter, create_router};
use luna_analyzer::view::mirrored_bands;
use ringbuf::{HeapRb, traits::Split};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RING_SIZE: usize = 8_192;

fn main() {
    // let host = cpal::default_host();
    // println!("Using Host: {:?}", host.id());
    //
    // let _initial_filter = std::env::args()
    //     .nth(1)
    //     .unwrap_or_else(|| "monitor".to_string());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("luna-analyzer")
            .with_min_inner_size([600.0, 200.0])
            .with_max_inner_size([1920.0, 1080.0])
            .with_inner_size([600.0, 200.0])
            .with_resizable(true)
            .with_transparent(true),

        ..Default::default()
    };

    let result = eframe::run_native(
        "Luna Analyzer Pipewire-test",
        options,
        Box::new(|_cc| {
            let app = PipewireAnalyzerApp::new().expect("Fail to Init Pipewire Analyzer App");
            Ok(Box::new(app))
        }),
        // Box::new(move |_cc| Ok(Box::new(AnalyzerApp::new(host, &initial_filter)))),
    );

    if let Err(err) = result {
        eprintln!("GUI error: {err}");
    }
}

struct PipewireAnalyzerApp {
    engine: PipewireEngine,
    pipeline_thread: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
    latest_bands: Arc<Mutex<Vec<f32>>>,
}

impl PipewireAnalyzerApp {
    fn new() -> Result<Self, String> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let latest_bands = Arc::new(Mutex::new(Vec::<f32>::with_capacity(NUM_BANDS)));
        let sink_metrics = Arc::new(SinkMetrics::default());
        let playback_metrics = Arc::new(PlaybackMetrics::default());

        let rb_in = HeapRb::new(RING_SIZE);
        let (in_producer, in_consumer) = rb_in.split();

        let rb_out = HeapRb::new(RING_SIZE * 2);
        let (out_producer, out_consumer) = rb_out.split();

        let mut engine = PipewireEngine::new();
        engine
            .start(
                in_producer,
                Some(out_consumer),
                Arc::clone(&sink_metrics),
                Arc::clone(&playback_metrics),
            )
            .expect("Fail to create Pipewire Node");

        let stop_for_worker = Arc::clone(&stop_flag);
        let bands_for_worker = Arc::clone(&latest_bands);

        let pipeline_thread = thread::Builder::new()
            .name("luna-analysis".into())
            .spawn(move || {
                let stereo_fx = StereoFxChain::new(FxChain::new(), FxChain::new());
                pipeline::run_audio_processor(
                    in_consumer,
                    Some(out_producer),
                    PIPEWIRE_SAMPLE_RATE,
                    stereo_fx,
                    stop_for_worker,
                    move |bands| {
                        if let Ok(mut latest) = bands_for_worker.try_lock() {
                            latest.clear();
                            latest.extend_from_slice(bands);
                        }
                    },
                    Arc::clone(&sink_metrics),
                    Arc::clone(&playback_metrics),
                );
            })
            .expect("Fail to init Analysis Thread");
        Ok(Self {
            engine,
            pipeline_thread: Some(pipeline_thread),
            stop_flag,
            latest_bands,
        })
    }
}

impl Drop for PipewireAnalyzerApp {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.engine.stop();
        if let Some(handle) = self.pipeline_thread.take() {
            let _ = handle.join();
        }
    }
}

impl eframe::App for PipewireAnalyzerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(8));

        let bands = self
            .latest_bands
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        let mirrored = mirrored_bands(&bands);

        let background = egui::Color32::from_rgba_unmultiplied(5, 12, 20, 180);
        let frame = egui::Frame::NONE.fill(background);

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let conf_button =
                        egui::Button::new(egui::RichText::new("⚙").color(egui::Color32::WHITE))
                            .fill(background);
                    if ui
                        .add(conf_button)
                        .on_hover_text("Configurations")
                        .clicked()
                    {
                        apply_custom_visuals(ui);
                        // self.show_settings = !self.show_settings;
                    }

                    let fx_button =
                        egui::Button::new(egui::RichText::new("FX").color(egui::Color32::WHITE))
                            .fill(background);
                    if ui.add(fx_button).on_hover_text("Effects").clicked() {
                        apply_custom_visuals(ui);
                        // self.show_effects = !self.show_effects;
                    }
                });
            });

            draw_mirrored_spectrum(ui, &mirrored);
        });
    }
}

// struct AnalyzerApp {
//     host: cpal::Host,
//
//     luna_router: Box<dyn AudioRouter>,
//
//     input_devices: Vec<cpal::Device>,
//     input_device_names: Vec<String>,
//     selected_input: Option<usize>,
//     output_devices: Vec<cpal::Device>,
//     output_device_names: Vec<String>,
//     selected_output: Option<usize>,
//
//     input_stream: Option<cpal::Stream>,
//     output_stream: Option<cpal::Stream>,
//
//     stop_flag: Arc<AtomicBool>,
//
//     vinyl_params: VinylParams,
//
//     latest_bands: Arc<Mutex<Vec<f32>>>,
//     show_settings: bool,
//     status_msg: Option<String>,
//
//     show_effects: bool,
// }
//
// impl AnalyzerApp {
//     fn new(host: cpal::Host, initial_filter: &str) -> Self {
//         let mut app = Self {
//             host,
//             luna_router: create_router(),
//             input_devices: Vec::new(),
//             input_device_names: Vec::new(),
//             selected_input: None,
//             output_devices: Vec::new(),
//             output_device_names: Vec::new(),
//             selected_output: None,
//             input_stream: None,
//             output_stream: None,
//             stop_flag: Arc::new(AtomicBool::new(false)),
//             vinyl_params: VinylParams {
//                 enabled: Arc::new(AtomicBool::new(false)),
//                 amount: Arc::new(AtomicU32::new(0.5f32.to_bits())),
//                 crackle: Arc::new(AtomicU32::new(0.25f32.to_bits())),
//                 hiss: Arc::new(AtomicU32::new(0.25f32.to_bits())),
//                 room: Arc::new(AtomicU32::new(0.25f32.to_bits())),
//                 wow: Arc::new(AtomicU32::new(0.45f32.to_bits())),
//                 warmth: Arc::new(AtomicU32::new(0.50f32.to_bits())),
//                 needle: Arc::new(AtomicU32::new(0.40f32.to_bits())),
//             },
//             latest_bands: Arc::new(Mutex::new(Vec::new())),
//             show_settings: false,
//             status_msg: None,
//             show_effects: false,
//         };
//
//         if let Err(e) = app.luna_router.setup() {
//             app.status_msg = Some(format!("Luna Routing Failed: {e}"));
//         }
//         app.refresh_input_devices();
//         app.refresh_output_devices();
//
//         let active_hint = app
//             .luna_router
//             .input_device_hint()
//             .map(|h| h.to_lowercase());
//
//         let cli_filter = initial_filter.to_lowercase();
//
//         let initial_input = active_hint
//             .and_then(|hint| {
//                 app.input_device_names
//                     .iter()
//                     .position(|n| n.to_lowercase().contains(&hint))
//             })
//             .or_else(|| {
//                 app.input_device_names
//                     .iter()
//                     .position(|n| n.to_lowercase().contains(&cli_filter))
//             })
//             .or_else(|| {
//                 app.input_device_names
//                     .iter()
//                     .position(|n| n.to_lowercase().contains("monitor"))
//             })
//             .or((!app.input_devices.is_empty()).then_some(0));
//
//         match initial_input {
//             Some(i) => app.start_pipeline(i),
//             None => app.status_msg = Some("No Device Found".into()),
//         }
//
//         app
//     }
//
//     fn refresh_input_devices(&mut self) {
//         match self.host.input_devices() {
//             Ok(iter) => {
//                 self.input_devices = iter.collect();
//                 self.input_device_names = self
//                     .input_devices
//                     .iter()
//                     .map(|d| {
//                         d.description()
//                             .map(|desc| desc.name().to_string())
//                             .unwrap_or_else(|_| "Unknown input Device".to_string())
//                     })
//                     .collect();
//                 if let Some(sel) = self.selected_input
//                     && sel >= self.input_device_names.len()
//                 {
//                     self.selected_input = None;
//                 }
//             }
//             Err(e) => self.status_msg = Some(format!("Fail to list Devices {e}")),
//         }
//     }
//
//     fn refresh_output_devices(&mut self) {
//         match self.host.output_devices() {
//             Ok(iter) => {
//                 self.output_devices = iter.collect();
//                 self.output_device_names = self
//                     .output_devices
//                     .iter()
//                     .map(|d| {
//                         d.description()
//                             .map(|desc| desc.name().to_string())
//                             .unwrap_or_else(|_| "Unknown output Device".to_string())
//                     })
//                     .collect();
//                 if let Some(sel) = self.selected_output
//                     && sel >= self.output_device_names.len()
//                 {
//                     self.selected_output = None;
//                 }
//             }
//             Err(e) => self.status_msg = Some(format!("Failt to list Output Device {e}")),
//         }
//     }
//
//     fn start_pipeline(&mut self, input_index: usize) {
//         self.stop_flag.store(true, Ordering::Relaxed);
//         self.input_stream = None;
//         self.output_stream = None;
//         if let Ok(mut bands) = self.latest_bands.lock() {
//             bands.clear();
//         }
//
//         //input
//         let Some(device) = self.input_devices.get(input_index) else {
//             self.status_msg = Some("Invalid input device.".into());
//             return;
//         };
//         let input_config = match device.default_input_config() {
//             Ok(c) => c,
//             Err(e) => {
//                 self.status_msg = Some(format!("Fail to get input config: {e}"));
//                 return;
//             }
//         };
//         let input_sample_rate = input_config.sample_rate();
//
//         let rb = HeapRb::new(RING_SIZE);
//         let (producer, consumer) = rb.split();
//
//         let stream = match audio::build_input_stream_for_device(device, input_config, producer) {
//             Ok(s) => s,
//             Err(e) => {
//                 self.status_msg = Some(format!("Fail to create input stream: {e}"));
//                 return;
//             }
//         };
//         if let Err(e) = stream.play() {
//             self.status_msg = Some(format!("Fail to start input stream: {e}"));
//             return;
//         }
//         self.input_stream = Some(stream);
//         self.selected_input = Some(input_index);
//
//         let mut left = luna_analyzer::fx::FxChain::new();
//         let mut right = luna_analyzer::fx::FxChain::new();
//
//         left.add(VinylEffect::new(
//             input_sample_rate,
//             self.vinyl_params.clone(),
//             0x9E3779B9,
//         ));
//
//         right.add(VinylEffect::new(
//             input_sample_rate,
//             self.vinyl_params.clone(),
//             0x12345678,
//         ));
//
//         let stereo_fx = luna_analyzer::fx::StereoFxChain::new(left, right);
//         //output
//         let mut out_producer = None;
//         if let Some(out_index) = self.selected_output
//             && let Some(out_device) = self.output_devices.get(out_index)
//         {
//             match out_device.default_output_config() {
//                 Ok(out_config) => {
//                     let rb_out = HeapRb::new(RING_SIZE * 2);
//                     let (prod, cons) = rb_out.split();
//
//                     match audio::build_output_stream_for_device(out_device, out_config, cons) {
//                         Ok(s) => match s.play() {
//                             Ok(()) => {
//                                 self.output_stream = Some(s);
//                                 out_producer = Some(prod);
//                             }
//                             Err(e) => self.status_msg = Some(format!("Fail to start output: {e}")),
//                         },
//                         Err(e) => self.status_msg = Some(format!("Fail to create output: {e}")),
//                     }
//                 }
//                 Err(e) => self.status_msg = Some(format!("Fail to get output config: {e}")),
//             }
//         }
//
//         self.stop_flag = Arc::new(AtomicBool::new(false));
//         let stop = self.stop_flag.clone();
//         let bands = self.latest_bands.clone();
//
//         thread::spawn(move || {
//             pipeline::run_audio_processor(
//                 consumer,
//                 out_producer,
//                 input_sample_rate,
//                 stereo_fx,
//                 stop,
//                 move |b| {
//                     if let Ok(mut g) = bands.lock() {
//                         g.clear();
//                         g.extend_from_slice(b);
//                     }
//                 },
//             );
//         });
//     }
//
//     fn restart(&mut self) {
//         if let Some(i) = self.selected_input {
//             self.start_pipeline(i);
//         }
//     }
// }
//
// impl Drop for AnalyzerApp {
//     fn drop(&mut self) {
//         self.stop_flag.store(true, Ordering::Relaxed);
//     }
// }
//
// impl eframe::App for AnalyzerApp {
//     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//         ui.ctx().request_repaint_after(Duration::from_millis(16));
//
//         let bands = self
//             .latest_bands
//             .lock()
//             .map(|g| g.clone())
//             .unwrap_or_default();
//         let mirrored = mirrored_bands(&bands);
//
//         let background = egui::Color32::from_rgba_unmultiplied(5, 12, 20, 180);
//         let frame = egui::Frame::NONE.fill(background);
//
//         egui::CentralPanel::default().frame(frame).show(ui, |ui| {
//             ui.horizontal(|ui| {
//                 ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
//                     let conf_button =
//                         egui::Button::new(egui::RichText::new("⚙").color(egui::Color32::WHITE))
//                             .fill(background);
//                     if ui
//                         .add(conf_button)
//                         .on_hover_text("Configurations")
//                         .clicked()
//                     {
//                         apply_custom_visuals(ui);
//                         self.show_settings = !self.show_settings;
//                     }
//
//                     let fx_button =
//                         egui::Button::new(egui::RichText::new("FX").color(egui::Color32::WHITE))
//                             .fill(background);
//                     if ui.add(fx_button).on_hover_text("Effects").clicked() {
//                         apply_custom_visuals(ui);
//                         self.show_effects = !self.show_effects;
//                     }
//                 });
//             });
//
//             draw_mirrored_spectrum(ui, &mirrored);
//
//             if self.show_settings {
//                 let mut input_pick = None;
//                 let mut output_pick: Option<Option<usize>> = None;
//                 let mut refresh = false;
//                 let mut quit = false;
//
//                 egui::Window::new("Configurations")
//                     .collapsible(false)
//                     .resizable(false)
//                     .show(ui.ctx(), |ui| {
//                         // Select Input Devices on GUI
//                         ui.label(egui::RichText::new("Input Devices").color(egui::Color32::WHITE));
//
//                         let current = self
//                             .selected_input
//                             .and_then(|i| self.input_device_names.get(i))
//                             .cloned()
//                             .unwrap_or_else(|| "None".to_string());
//
//                         egui::ComboBox::from_id_salt("device_picker")
//                             .selected_text(current)
//                             .show_ui(ui, |ui| {
//                                 for (i, name) in self.input_device_names.iter().enumerate() {
//                                     let is_selected = self.selected_input == Some(i);
//
//                                     let label_text =
//                                         egui::RichText::new(name).color(if is_selected {
//                                             egui::Color32::from_rgb(250, 35, 150)
//                                         } else {
//                                             egui::Color32::WHITE
//                                         });
//
//                                     if ui.selectable_label(is_selected, label_text).clicked() {
//                                         input_pick = Some(i);
//                                     }
//                                 }
//                             });
//                         ui.separator();
//                         // Select Ouput Devices on GUI
//                         ui.label("Output Devices:");
//
//                         let current_out = self
//                             .selected_output
//                             .and_then(|i| self.output_device_names.get(i))
//                             .cloned()
//                             .unwrap_or_else(|| "Disabled".to_string());
//
//                         egui::ComboBox::from_id_salt("output_picker")
//                             .selected_text(current_out)
//                             .show_ui(ui, |ui| {
//                                 if ui
//                                     .selectable_label(
//                                         self.selected_output.is_none(),
//                                         "Disabled (analyzer only)",
//                                     )
//                                     .clicked()
//                                 {
//                                     output_pick = Some(None);
//                                 }
//                                 for (i, name) in self.output_device_names.iter().enumerate() {
//                                     if ui
//                                         .selectable_label(self.selected_output == Some(i), name)
//                                         .clicked()
//                                     {
//                                         output_pick = Some(Some(i));
//                                     }
//                                 }
//                             });
//
//                         let update_button = egui::Button::new(
//                             egui::RichText::new("Update list").color(egui::Color32::WHITE),
//                         )
//                         .fill(background);
//                         if ui.add(update_button).clicked() {
//                             refresh = true;
//                         }
//
//                         if let Some(msg) = &self.status_msg {
//                             ui.colored_label(egui::Color32::from_rgb(150, 60, 50), msg);
//                         }
//
//                         ui.separator();
//
//                         let quit_button = egui::Button::new(
//                             egui::RichText::new("Quit").color(egui::Color32::WHITE),
//                         )
//                         .fill(background);
//                         if ui.add(quit_button).clicked() {
//                             quit = true;
//                         }
//                     });
//
//                 if refresh {
//                     self.refresh_input_devices();
//                     self.refresh_output_devices();
//                 }
//                 if let Some(i) = input_pick
//                     && self.selected_input != Some(i)
//                 {
//                     self.start_pipeline(i);
//                 }
//                 if let Some(choice) = output_pick
//                     && self.selected_output != choice
//                 {
//                     self.selected_output = choice;
//                     if choice.is_some() {
//                         self.restart();
//                     } else {
//                         self.output_stream = None;
//                     }
//                 }
//                 if quit {
//                     ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
//                 }
//             }
//             // Effects(VinylEffect)
//             if self.show_effects {
//                 egui::Window::new("Effects")
//                     .collapsible(false)
//                     .resizable(false)
//                     .show(ui.ctx(), |ui| {
//                         egui::CollapsingHeader::new("Vinyl Simulation")
//                             .default_open(true)
//                             .show(ui, |ui| {
//                                 let mut enabled = self.vinyl_params.enabled.load(Ordering::Relaxed);
//                                 if ui.checkbox(&mut enabled, "Enabled").changed() {
//                                     self.vinyl_params.enabled.store(enabled, Ordering::Relaxed);
//                                 }
//                                 ui.add_enabled_ui(enabled, |ui| {
//                                     let mut amount = f32::from_bits(
//                                         self.vinyl_params.amount.load(Ordering::Relaxed),
//                                     );
//                                     let amount_slider = egui::Slider::new(&mut amount, 0.0..=1.0)
//                                         .text("Amount")
//                                         .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
//                                     if ui.add(amount_slider).changed() {
//                                         self.vinyl_params
//                                             .amount
//                                             .store(amount.to_bits(), Ordering::Relaxed);
//                                     }
//                                     let mut crackle = f32::from_bits(
//                                         self.vinyl_params.crackle.load(Ordering::Relaxed),
//                                     );
//                                     let crackle_slider = egui::Slider::new(&mut crackle, 0.0..=1.0)
//                                         .text("Crackle")
//                                         .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
//                                     if ui.add(crackle_slider).changed() {
//                                         self.vinyl_params
//                                             .crackle
//                                             .store(crackle.to_bits(), Ordering::Relaxed);
//                                     }
//                                     // let mut hiss = f32::from_bits(
//                                     //     self.vinyl_params.hiss.load(Ordering::Relaxed),
//                                     // );
//                                     // let hiss_slider = egui::Slider::new(&mut hiss, 0.0..=1.0)
//                                     //     .text("Hiss")
//                                     //     .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
//                                     // if ui.add(hiss_slider).changed() {
//                                     //     self.vinyl_params
//                                     //         .hiss
//                                     //         .store(hiss.to_bits(), Ordering::Relaxed);
//                                     // }
//                                     let mut room = f32::from_bits(
//                                         self.vinyl_params.room.load(Ordering::Relaxed),
//                                     );
//                                     let room_slider = egui::Slider::new(&mut room, 0.0..=1.0)
//                                         .text("RoomNoise")
//                                         .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
//                                     if ui.add(room_slider).changed() {
//                                         self.vinyl_params
//                                             .room
//                                             .store(room.to_bits(), Ordering::Relaxed);
//                                     }
//                                     let mut warmth = f32::from_bits(
//                                         self.vinyl_params.warmth.load(Ordering::Relaxed),
//                                     );
//                                     let warmth_slider = egui::Slider::new(&mut warmth, 0.0..=1.0)
//                                         .text("Warmth")
//                                         .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
//                                     if ui.add(warmth_slider).changed() {
//                                         self.vinyl_params
//                                             .warmth
//                                             .store(warmth.to_bits(), Ordering::Relaxed);
//                                     }
//                                 });
//                             });
//                     });
//             }
//         });
//     }
// }
//
fn apply_custom_visuals(ui: &mut egui::Ui) {
    let mut visuals = egui::Visuals::dark();
    let bg = egui::Color32::from_rgba_unmultiplied(5, 12, 20, 180);
    visuals.window_fill = bg;
    visuals.panel_fill = bg;
    visuals.widgets.inactive.weak_bg_fill = bg;
    visuals.widgets.open.bg_fill = bg;
    visuals.widgets.hovered.bg_fill = bg;
    ui.ctx().set_visuals(visuals);
}

fn draw_mirrored_spectrum(ui: &mut egui::Ui, mirrored: &[f32]) {
    let rect = ui.max_rect();
    let painter = ui.painter();

    if mirrored.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Waiting for audio...",
            egui::FontId::monospace(14.0),
            egui::Color32::from_rgb(255, 255, 255),
        );
        return;
    }

    let size_t = ((170.0 - rect.height()) / 20.0).clamp(0.0, 1.0);
    let center_y = rect.center().y;
    //Bars max Height
    let max_height = rect.height() * (0.40 - 0.10 * size_t);

    let margin_x = 24.0;

    let count = mirrored.len();
    let available_width = rect.width() - margin_x * 2.0;
    let gap = (available_width / count as f32 * 0.25).clamp(1.0, 4.0);
    let total_gap = gap * (count.saturating_sub(1) as f32).max(1.0);

    let bar_width = ((available_width - total_gap) / count as f32).max(1.0);

    // gamma and gain control the bars "energy"
    // +GAIN = +Bars go UP
    // GAMMA is the GAIN expoent.
    let gamma = 1.05 + 0.30 * size_t;
    let gain = 0.9 + 0.30 * size_t;

    // All bars colors
    let bar_color = egui::Color32::from_rgba_unmultiplied(237, 204, 254, 255);
    let deep_shadow = egui::Color32::from_rgba_unmultiplied(12, 8, 20, 255);
    let contrast_color = egui::Color32::from_rgba_unmultiplied(247, 69, 255, 255);
    let highlight_color = egui::Color32::from_rgba_unmultiplied(110, 255, 70, 255);
    // let cap_color = egui::Color32::from_rgba_unmultiplied(242, 226, 45, 200);

    for (i, value) in mirrored.iter().enumerate() {
        let linear = value.clamp(0.0, 1.0);
        let v = (linear * gain).clamp(0.0, 1.0).powf(gamma);
        let height = max_height * v;

        if height > 0.5 {
            let x = rect.left() + margin_x + i as f32 * (bar_width + gap);

            let top = center_y - height;
            let bottom = center_y + height;
            let bar_rect =
                egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + bar_width, bottom));

            let shadow_offset = egui::vec2(4.0, 4.0);

            // First layer, give some contrast with the principal bar
            let diffuse_rect = bar_rect.translate(shadow_offset).expand(1.5);
            painter.rect_filled(diffuse_rect, egui::CornerRadius::ZERO, contrast_color);

            // Second layer, the "real" shadow
            let direct_rect = bar_rect.translate(shadow_offset);
            painter.rect_filled(direct_rect, egui::CornerRadius::ZERO, deep_shadow);

            // Principal bar
            painter.rect_filled(bar_rect, egui::CornerRadius::ZERO, bar_color);

            painter.line_segment(
                [egui::pos2(x + 0.5, top), egui::pos2(x + 0.5, bottom)],
                egui::Stroke::new(1.4, highlight_color),
            );

            // let cap = 1.0;
            // painter.rect_filled(
            //     egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + bar_width, top + cap)),
            //     egui::CornerRadius::ZERO,
            //     cap_color,
            // );
            // painter.rect_filled(
            //     egui::Rect::from_min_max(
            //         egui::pos2(x, bottom - cap),
            //         egui::pos2(x + bar_width, bottom),
            //     ),
            //     egui::CornerRadius::ZERO,
            //     cap_color,
            // );
        }
    }
}
