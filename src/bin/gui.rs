use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui::{self};
use luna_analyzer::audio::build_stream_for_device;
use luna_analyzer::pipeline;
use luna_analyzer::view::mirrored_bands;
use ringbuf::{HeapRb, traits::Split};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const RING_SIZE: usize = 16384;

fn main() {
    let host = cpal::default_host();
    println!("Using Host: {:?}", host.id());

    let initial_filter = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "monitor".to_string());

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
        "Luna Analyzer",
        options,
        Box::new(move |_cc| Ok(Box::new(AnalyzerApp::new(host, &initial_filter)))),
    );

    if let Err(err) = result {
        eprintln!("GUI error: {err}");
    }
}

struct AnalyzerApp {
    host: cpal::Host,
    devices: Vec<cpal::Device>,
    device_names: Vec<String>,
    selected: Option<usize>,

    stream: Option<cpal::Stream>,
    stop_flag: Arc<AtomicBool>,

    latest_bands: Arc<Mutex<Vec<f32>>>,
    show_settings: bool,
    status_msg: Option<String>,
}

impl AnalyzerApp {
    fn new(host: cpal::Host, initial_filter: &str) -> Self {
        let mut app = Self {
            host,
            devices: Vec::new(),
            device_names: Vec::new(),
            selected: None,
            stream: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            latest_bands: Arc::new(Mutex::new(Vec::new())),
            show_settings: false,
            status_msg: None,
        };

        app.refresh_devices();

        let filter = initial_filter.to_lowercase();
        let initial = app
            .device_names
            .iter()
            .position(|n| n.to_lowercase().contains(&filter))
            .or((!app.devices.is_empty()).then_some(0));

        match initial {
            Some(i) => app.start_capture(i),
            None => app.status_msg = Some("No Device Found".into()),
        }

        app
    }

    fn refresh_devices(&mut self) {
        match self.host.input_devices() {
            Ok(iter) => {
                self.devices = iter.collect();
                self.device_names = self
                    .devices
                    .iter()
                    .map(|d| {
                        d.description()
                            .map(|desc| desc.name().to_string())
                            .unwrap_or_else(|_| "Unknown Device".to_string())
                    })
                    .collect();
            }
            Err(e) => self.status_msg = Some(format!("Fail to list Devices {e}")),
        }
    }

    fn start_capture(&mut self, index: usize) {
        // 1. Mata a captura anterior: sinaliza a thread e derruba o stream
        self.stop_flag.store(true, Ordering::Relaxed);
        self.stream = None;

        if let Ok(mut bands) = self.latest_bands.lock() {
            bands.clear();
        }

        let Some(device) = self.devices.get(index) else {
            return;
        };

        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                self.status_msg = Some(format!("Fail to get config: {e}"));
                return;
            }
        };
        let sample_rate = config.sample_rate();

        let rb = HeapRb::new(RING_SIZE);
        let (producer, consumer) = rb.split();

        let stream = match build_stream_for_device(device, config, producer) {
            Ok(s) => s,
            Err(e) => {
                self.status_msg = Some(format!("Fail to create stream: {e}"));
                return;
            }
        };

        if let Err(e) = stream.play() {
            self.status_msg = Some(format!("Fail to init stream: {e}"));
            return;
        }

        self.stream = Some(stream);
        self.selected = Some(index);
        self.status_msg = None;

        // 2. Nova flag + nova thread de análise pra essa captura
        self.stop_flag = Arc::new(AtomicBool::new(false));
        let stop = self.stop_flag.clone();
        let bands = self.latest_bands.clone();

        thread::spawn(move || {
            pipeline::run_analysis_stoppable(consumer, sample_rate, stop, move |b| {
                if let Ok(mut g) = bands.lock() {
                    g.clear();
                    g.extend_from_slice(b);
                }
            });
        });

        if let Some(name) = self.device_names.get(index) {
            println!(">>> Using Device: {name}");
        }
    }
}

impl Drop for AnalyzerApp {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

impl eframe::App for AnalyzerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(16));

        let bands = self
            .latest_bands
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let mirrored = mirrored_bands(&bands);

        let background = egui::Color32::from_rgba_unmultiplied(26, 36, 120, 50);
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
                        let mut visuals = egui::Visuals::dark();
                        visuals.window_fill =
                            egui::Color32::from_rgba_unmultiplied(26, 36, 120, 50);
                        visuals.panel_fill = visuals.window_fill;
                        visuals.widgets.inactive.weak_bg_fill =
                            egui::Color32::from_rgba_unmultiplied(26, 36, 120, 50);
                        visuals.widgets.open.bg_fill =
                            egui::Color32::from_rgba_unmultiplied(26, 36, 120, 50);
                        visuals.widgets.hovered.bg_fill =
                            egui::Color32::from_rgba_unmultiplied(26, 36, 120, 50);
                        ui.ctx().set_visuals(visuals);
                        self.show_settings = !self.show_settings;
                    }
                });
            });

            draw_mirrored_spectrum(ui, &mirrored);

            if self.show_settings {
                let mut pick = None;
                let mut refresh = false;
                let mut quit = false;

                egui::Window::new("Configurations")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        ui.label(egui::RichText::new("Input Devices").color(egui::Color32::WHITE));

                        let current = self
                            .selected
                            .and_then(|i| self.device_names.get(i))
                            .cloned()
                            .unwrap_or_else(|| "None".to_string());

                        egui::ComboBox::from_id_salt("device_picker")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (i, name) in self.device_names.iter().enumerate() {
                                    let is_selected = self.selected == Some(i);

                                    let label_text =
                                        egui::RichText::new(name).color(if is_selected {
                                            egui::Color32::from_rgb(250, 35, 150)
                                        } else {
                                            egui::Color32::WHITE
                                        });

                                    if ui.selectable_label(is_selected, label_text).clicked() {
                                        pick = Some(i);
                                    }
                                }
                            });
                        let update_button = egui::Button::new(
                            egui::RichText::new("Update list").color(egui::Color32::WHITE),
                        )
                        .fill(background);
                        if ui.add(update_button).clicked() {
                            refresh = true;
                        }

                        if let Some(msg) = &self.status_msg {
                            ui.colored_label(egui::Color32::from_rgb(150, 60, 50), msg);
                        }

                        ui.separator();

                        let quit_button = egui::Button::new(
                            egui::RichText::new("Quit").color(egui::Color32::WHITE),
                        )
                        .fill(background);
                        if ui.add(quit_button).clicked() {
                            quit = true;
                        }
                    });

                if refresh {
                    self.refresh_devices();
                }
                if let Some(i) = pick
                    && self.selected != Some(i)
                {
                    self.start_capture(i);
                }
                if quit {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
    }
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
    let max_height = rect.height() * (0.38 - 0.10 * size_t);

    let margin_x = 24.0;

    let count = mirrored.len();
    let available_width = rect.width() - margin_x * 2.0;
    let gap = (available_width / count as f32 * 0.25).clamp(1.0, 4.0);
    let total_gap = gap * (count.saturating_sub(1) as f32).max(1.0);

    let bar_width = ((available_width - total_gap) / count as f32).max(1.0);

    // gamma and gain control the bars "energy"
    // +energy = -gamma and/or +gain(gamma MIN  0.5 | gain MAX 1.5)
    // -energy = +gamma and/or -gain(gamma MAX  1.0 | gain MIN 1.0)
    let gamma = 0.70 + 0.30 * size_t;
    let gain = 1.20 + 0.30 * size_t;

    // All bars colors
    let bar_color = egui::Color32::from_rgba_unmultiplied(237, 214, 254, 255);
    let deep_shadow = egui::Color32::from_rgba_unmultiplied(10, 25, 44, 220);
    let soft_shadow = egui::Color32::from_rgba_unmultiplied(240, 112, 44, 230);
    let highlight_color = egui::Color32::from_rgba_unmultiplied(43, 70, 240, 220);
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
            painter.rect_filled(diffuse_rect, egui::CornerRadius::ZERO, soft_shadow);

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
