use eframe::egui;
use luna_analyzer::audio_capture::{
    INPUT_CAPACITY, OUTPUT_CAPACITY, PIPEWIRE_SAMPLE_RATE, PipewireEngine, create_pipes,
};
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
const SMALL_ATTACK_TAU: f32 = 0.012;
const RELEASE_TAU: f32 = 0.035;
const STRONG_RISE: f32 = 0.06;
const GAIN: f32 = 1.10;
const GAMMA: f32 = 0.80;
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
        "Luna Analyzer Pipewire-test",
        options,
        Box::new(|_cc| {
            let app = PipewireAnalyzerApp::new().expect("Fail to Init Pipewire Analyzer App");
            Ok(Box::new(app))
        }),
    );

    if let Err(err) = result {
        eprintln!("GUI error: {err}");
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

struct PipewireAnalyzerApp {
    engine: PipewireEngine,
    pipeline_thread: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
    frames: FrameQueue,
    shown: [f32; NUM_BANDS],
    latest: [f32; NUM_BANDS],
    last_frame_at: Instant,
    cleared: bool,
}

impl PipewireAnalyzerApp {
    fn new() -> Result<Self, String> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let frames = new_frame_queue();

        let (capture, process, playback) = create_pipes(INPUT_CAPACITY, OUTPUT_CAPACITY);
        let chunk_hint = Arc::new(AtomicUsize::new(0));

        let mut engine = PipewireEngine::new();
        engine
            .start(capture, Some(playback), Arc::clone(&chunk_hint))
            .expect("Fail to init Pipewire");

        let stereo_fx = StereoFxChain::new(FxChain::new(), FxChain::new());
        let stop_for_worker = Arc::clone(&stop_flag);
        let frames_for_worker = Arc::clone(&frames);
        let hint_for_worker = Arc::clone(&chunk_hint);

        let pipeline_thread = thread::Builder::new()
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
            })
            .map_err(|error| format!("Fail to start analysis thread: {error}"))?;

        Ok(Self {
            engine,
            pipeline_thread: Some(pipeline_thread),
            stop_flag,
            frames,
            shown: [0.0; NUM_BANDS],
            latest: [0.0; NUM_BANDS],
            last_frame_at: Instant::now(),
            cleared: true,
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
        ui.ctx().request_repaint();

        let now = Instant::now();
        if let Some(bands) = take_due_frame(&self.frames, now) {
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
                    }

                    let fx_button =
                        egui::Button::new(egui::RichText::new("FX").color(egui::Color32::WHITE))
                            .fill(background);
                    if ui.add(fx_button).on_hover_text("Effects").clicked() {
                        apply_custom_visuals(ui);
                    }
                });
            });

            draw_mirrored_spectrum(ui, &mirrored);
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
    let gap = (available_width / count as f32 * 0.20).clamp(1.0, 3.6);
    let total_gap = gap * (count.saturating_sub(1) as f32).max(1.0);

    let bar_width = ((available_width - total_gap) / count as f32).max(1.0);

    // gamma and gain control the bars "energy"
    // +GAIN = +Bars go UP
    // GAMMA is the GAIN expoent.
    let gamma = 1.0 + 0.30 * size_t;
    let gain = 1.0 + 0.30 * size_t;

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
