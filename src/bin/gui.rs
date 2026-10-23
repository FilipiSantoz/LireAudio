use eframe::egui;
use luna_analyzer::audio_capture::{
    INPUT_CAPACITY, OUTPUT_CAPACITY, PIPEWIRE_SAMPLE_RATE, PipewireEngine, create_pipes,
};
use luna_analyzer::fx::VinylEffect;
use luna_analyzer::fx::vinyl::VinylParams;
use luna_analyzer::fx::{FxChain, StereoFxChain};
use luna_analyzer::pipeline::{
    AudioProcessor, FrameQueue, NUM_BANDS, new_frame_queue, take_due_frame,
};
use luna_analyzer::view::mirrored_bands;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const NOISE_FLOOR: f32 = 0.01;
const SMALL_ATTACK_TAU: f32 = 0.022;
const RELEASE_TAU: f32 = 0.033;
const STRONG_RISE: f32 = 0.06;
// gamma and gain control the bars "energy"
// +GAIN = +Bars go UP
// GAMMA is the GAIN expoent.
const GAIN: f32 = 1.10;
const GAMMA: f32 = 1.21;
const STALL_TIMEOUT: Duration = Duration::from_millis(80);

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("luna-analyzer")
            .with_min_inner_size([600.0, 200.0])
            .with_max_inner_size([1920.0, 1080.0])
            .with_inner_size([700.0, 220.0])
            .with_resizable(true)
            .with_transparent(true),
        ..Default::default()
    };

    let result = eframe::run_native(
        "Luna Analyzer",
        options,
        Box::new(|_cc| {
            let app = AnalyzerApp::new();
            Ok(Box::new(app))
        }),
    );

    if let Err(err) = result {
        eprintln!("GUI error: {err}");
    }
}

struct AudioSession {
    pw_engine: PipewireEngine,
    pipeline_thread: Option<JoinHandle<()>>,
    stop_flag: Option<Arc<AtomicBool>>,
    frames: FrameQueue,
}

struct AnalyzerApp {
    audio: AudioSession,
    shown: [f32; NUM_BANDS],
    latest: [f32; NUM_BANDS],
    vinyl_params: VinylParams,
    last_frame_at: Instant,
    cleared: bool,
    show_settings: bool,
    show_effects: bool,
}

impl AnalyzerApp {
    fn new() -> Self {
        let vinyl_params = VinylParams::default();
        vinyl_params.enabled.store(false, Ordering::Relaxed);
        let mut app = Self {
            audio: AudioSession::new(),
            vinyl_params: vinyl_params.clone(),
            shown: [0.0; NUM_BANDS],
            latest: [0.0; NUM_BANDS],
            last_frame_at: Instant::now(),
            cleared: true,
            show_settings: false,
            show_effects: false,
        };
        let _ = app.audio.start(vinyl_params);
        app
    }
}

impl AudioSession {
    fn new() -> Self {
        let pw_engine = PipewireEngine::new();
        let frames = new_frame_queue();
        Self {
            pw_engine,
            pipeline_thread: None,
            stop_flag: None,
            frames,
        }
    }

    fn stop(&mut self) {
        if let Some(stop) = self.stop_flag.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.pw_engine.stop();
        if let Some(handle) = self.pipeline_thread.take() {
            let _ = handle.join();
        }
    }
    fn start(&mut self, vinyl_params: VinylParams) -> Result<(), String> {
        self.stop();

        let stop = Arc::new(AtomicBool::new(false));
        let frames = Arc::clone(&self.frames);
        let chunk_hint = Arc::new(AtomicUsize::new(0));

        let (capture, process, playback) = create_pipes(INPUT_CAPACITY, OUTPUT_CAPACITY);
        self.pw_engine
            .start(capture, Some(playback), Arc::clone(&chunk_hint))?;

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

        let pipeline_thread =
            match thread::Builder::new()
                .name("luna-analysis".into())
                .spawn(move || {
                    AudioProcessor {
                        process,
                        sample_rate: PIPEWIRE_SAMPLE_RATE,
                        stereo_fx,
                        stop: stop_for_worker,
                        frames: frames_for_worker,
                        chunk_hint: hint_for_worker,
                    }
                    .run();
                }) {
                Ok(handle) => handle,
                Err(e) => {
                    self.pw_engine.stop();
                    return Err(format!("Fail to init Pipeline Thread: {e}"));
                }
            };

        self.stop_flag = Some(stop);
        self.pipeline_thread = Some(pipeline_thread);
        Ok(())
    }
}

impl Drop for AudioSession {
    fn drop(&mut self) {
        self.stop();
    }
}

impl eframe::App for AnalyzerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();

        let now = Instant::now();
        if let Some(bands) = take_due_frame(&self.audio.frames, now) {
            self.latest = bands;
            self.last_frame_at = now;
            self.cleared = false;
        } else if !self.cleared && now.duration_since(self.last_frame_at) > STALL_TIMEOUT {
            self.latest = [0.0; NUM_BANDS];
            self.cleared = true;
        }
        let dt = ui.input(|i| i.stable_dt).min(0.048);
        update_envelope(&mut self.shown, &self.latest, dt);

        let mut gained = [0.0f32; NUM_BANDS];
        for (g, &s) in gained.iter_mut().zip(&self.shown) {
            *g = (s * GAIN).min(1.0).powf(GAMMA);
        }

        let mirrored = mirrored_bands(&gained);

        let background = egui::Color32::from_rgba_unmultiplied(5, 12, 20, 120);
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
                        self.show_settings = !self.show_settings;
                    }

                    let fx_button =
                        egui::Button::new(egui::RichText::new("FX").color(egui::Color32::WHITE))
                            .fill(background);
                    if ui.add(fx_button).on_hover_text("Effects").clicked() {
                        apply_custom_visuals(ui);
                        self.show_effects = !self.show_effects;
                    }
                });
            });
            draw_mirrored_spectrum(ui, &mirrored);

            if self.show_settings {
                let mut quit = false;
                egui::Window::new("Configurations")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        let quit_button = egui::Button::new(
                            egui::RichText::new("Quit").color(egui::Color32::WHITE),
                        )
                        .fill(background);
                        if ui.add(quit_button).clicked() {
                            quit = true;
                        }
                        if quit {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
            }
            if self.show_effects {
                egui::Window::new("Effects")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        egui::CollapsingHeader::new("Vinyl Simulation")
                            .default_open(true)
                            .show(ui, |ui| {
                                let mut enabled = self.vinyl_params.enabled.load(Ordering::Relaxed);
                                if ui.checkbox(&mut enabled, "Enabled").changed() {
                                    self.vinyl_params.enabled.store(enabled, Ordering::Relaxed);
                                }
                                ui.add_enabled_ui(enabled, |ui| {
                                    let mut amount = f32::from_bits(
                                        self.vinyl_params.amount.load(Ordering::Relaxed),
                                    );
                                    let amount_slider = egui::Slider::new(&mut amount, 0.0..=1.0)
                                        .text("Amount")
                                        .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
                                    if ui.add(amount_slider).changed() {
                                        self.vinyl_params
                                            .amount
                                            .store(amount.to_bits(), Ordering::Relaxed);
                                    }
                                    let mut crackle = f32::from_bits(
                                        self.vinyl_params.crackle.load(Ordering::Relaxed),
                                    );
                                    let crackle_slider = egui::Slider::new(&mut crackle, 0.0..=1.0)
                                        .text("Crackle")
                                        .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
                                    if ui.add(crackle_slider).changed() {
                                        self.vinyl_params
                                            .crackle
                                            .store(crackle.to_bits(), Ordering::Relaxed);
                                    }
                                    // let mut hiss = f32::from_bits(
                                    //     self.vinyl_params.hiss.load(Ordering::Relaxed),
                                    // );
                                    // let hiss_slider = egui::Slider::new(&mut hiss, 0.0..=1.0)
                                    //     .text("Hiss")
                                    //     .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
                                    // if ui.add(hiss_slider).changed() {
                                    //     self.vinyl_params
                                    //         .hiss
                                    //         .store(hiss.to_bits(), Ordering::Relaxed);
                                    // }
                                    let mut room = f32::from_bits(
                                        self.vinyl_params.room.load(Ordering::Relaxed),
                                    );
                                    let room_slider = egui::Slider::new(&mut room, 0.0..=1.0)
                                        .text("RoomNoise")
                                        .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
                                    if ui.add(room_slider).changed() {
                                        self.vinyl_params
                                            .room
                                            .store(room.to_bits(), Ordering::Relaxed);
                                    }
                                    let mut warmth = f32::from_bits(
                                        self.vinyl_params.warmth.load(Ordering::Relaxed),
                                    );
                                    let warmth_slider = egui::Slider::new(&mut warmth, 0.0..=1.0)
                                        .text("Warmth")
                                        .custom_formatter(|val, _| format!("{:.0}%", val * 100.0));
                                    if ui.add(warmth_slider).changed() {
                                        self.vinyl_params
                                            .warmth
                                            .store(warmth.to_bits(), Ordering::Relaxed);
                                    }
                                });
                            });
                    });
            }
        });
    }
}
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
            "...",
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

    // All bars colors
    let bar_color = egui::Color32::from_rgba_unmultiplied(237, 204, 254, 255);
    let deep_shadow = egui::Color32::from_rgba_unmultiplied(12, 8, 20, 255);
    let contrast_color = egui::Color32::from_rgba_unmultiplied(247, 69, 255, 255);
    let highlight_color = egui::Color32::from_rgba_unmultiplied(110, 255, 70, 255);

    for (i, value) in mirrored.iter().enumerate() {
        let linear = value.clamp(0.0, 1.0);
        let height = max_height * linear;

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
                egui::Stroke::new(1.94, highlight_color),
            );

            // let cap = 1.0;
            // painter.rect_filled(
            //     egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + bar_width, top + cap)),
            //     egui::CornerRadius::ZERO,
            //     highlight_color,
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

fn update_envelope(shown: &mut [f32], bands: &[f32], dt: f32) {
    let attack_k = 1.0 - (-dt / SMALL_ATTACK_TAU).exp();
    let release_k = 1.0 - (-dt / RELEASE_TAU).exp();

    for (s, &raw) in shown.iter_mut().zip(bands) {
        let value = if raw.is_finite() {
            raw.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let target = if value <= NOISE_FLOOR {
            0.0
        } else {
            (value - NOISE_FLOOR) / (1.0 - NOISE_FLOOR)
        };

        let delta = target - *s;
        if delta >= STRONG_RISE {
            *s = target;
        } else if delta > 0.0 {
            *s += delta * attack_k;
        } else {
            *s += delta * release_k;
        }
    }
}
