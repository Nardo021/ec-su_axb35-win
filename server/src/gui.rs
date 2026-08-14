use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use image::GenericImageView;

use crate::i18n::{t, Language};
use crate::platform::set_start_with_windows;
use crate::session::{AppSession, MetricsSnapshot};
use crate::tray::TrayEvent;

const ICON_BYTES: &[u8] = include_bytes!("../assets/ec-su_axb35-win.png");
const COG_ICON_BYTES: &[u8] = include_bytes!("../assets/cog.png");
const CHECK_ICON_BYTES: &[u8] = include_bytes!("../assets/check.png");
const CHART_HISTORY_SIZE: usize = 60;

#[derive(Clone, Debug)]
struct EditState {
    apu_edit: bool,
    fan_edit: [bool; 3],
    apu_applying: bool,
    fan_applying: [bool; 3],
    temp_power_mode: String,
    temp_fan_mode: [String; 3],
    temp_fan_level: [i32; 3],
    temp_fan_rampup: [String; 3],
    temp_fan_rampdown: [String; 3],
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            apu_edit: false,
            fan_edit: [false; 3],
            apu_applying: false,
            fan_applying: [false; 3],
            temp_power_mode: "balanced".into(),
            temp_fan_mode: ["auto".into(), "auto".into(), "auto".into()],
            temp_fan_level: [0, 0, 0],
            temp_fan_rampup: [
                "60,70,83,95,97".into(),
                "60,70,83,95,97".into(),
                "20,60,83,95,97".into(),
            ],
            temp_fan_rampdown: [
                "40,50,80,94,96".into(),
                "40,50,80,94,96".into(),
                "0,50,80,94,96".into(),
            ],
        }
    }
}

impl EditState {
    fn other_edit_active(&self, except_fan: Option<usize>) -> bool {
        if except_fan.is_some() && self.apu_edit {
            return true;
        }
        self.fan_edit
            .iter()
            .enumerate()
            .any(|(index, flag)| *flag && Some(index) != except_fan)
    }
}

struct ChartData {
    temperature: VecDeque<i32>,
    fans: [VecDeque<i32>; 3],
}

impl ChartData {
    fn new() -> Self {
        Self {
            temperature: VecDeque::with_capacity(CHART_HISTORY_SIZE),
            fans: [
                VecDeque::with_capacity(CHART_HISTORY_SIZE),
                VecDeque::with_capacity(CHART_HISTORY_SIZE),
                VecDeque::with_capacity(CHART_HISTORY_SIZE),
            ],
        }
    }

    fn push(&mut self, metrics: &MetricsSnapshot) {
        push_history(&mut self.temperature, i32::from(metrics.temperature));
        for (index, fan) in metrics.fans.iter().enumerate() {
            push_history(&mut self.fans[index], i32::from(fan.rpm));
        }
    }
}

fn push_history(history: &mut VecDeque<i32>, value: i32) {
    if history.len() >= CHART_HISTORY_SIZE {
        history.pop_front();
    }
    history.push_back(value);
}

struct UiState {
    session: Arc<AppSession>,
    firmware: Option<String>,
    metrics: Option<MetricsSnapshot>,
    error: Option<String>,
    error_at: Option<Instant>,
    edit: EditState,
    charts: ChartData,
    cog: Option<egui::TextureHandle>,
    check: Option<egui::TextureHandle>,
}

struct ControlApp {
    state: Arc<Mutex<UiState>>,
    stop: Arc<AtomicBool>,
    window_configured: bool,
    last_height: f32,
    tray_rx: Receiver<TrayEvent>,
}

pub fn run(session: Arc<AppSession>, tray_rx: Receiver<TrayEvent>) -> Result<(), String> {
    let firmware = session.firmware_version().ok();
    let state = Arc::new(Mutex::new(UiState {
        session: Arc::clone(&session),
        firmware,
        metrics: None,
        error: None,
        error_at: None,
        edit: EditState::default(),
        charts: ChartData::new(),
        cog: None,
        check: None,
    }));

    let stop = Arc::new(AtomicBool::new(false));
    {
        let state = Arc::clone(&state);
        let stop = Arc::clone(&stop);
        let session = Arc::clone(&session);
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match session.metrics() {
                    Ok(metrics) => {
                        let mut ui = state.lock().unwrap();
                        ui.charts.push(&metrics);
                        ui.metrics = Some(metrics);
                    }
                    Err(error) => {
                        let mut ui = state.lock().unwrap();
                        ui.error = Some(error);
                        ui.error_at = Some(Instant::now());
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    let icon = image::load_from_memory(ICON_BYTES).ok().map(|img| {
        let rgba = img.to_rgba8();
        let (width, height) = img.dimensions();
        egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }
    });

    let app = ControlApp {
        state,
        stop,
        window_configured: false,
        last_height: 0.0,
        tray_rx,
    };
    let title = t(session.config.lock().unwrap().language(), "app_title");

    eframe::run_native(
        title,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(title)
                .with_maximize_button(false)
                .with_icon(icon.unwrap_or_default()),
            ..Default::default()
        },
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .map_err(|error| format!("Failed to start GUI: {error}"))
}

impl eframe::App for ControlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.tray_rx.try_recv() {
            match event {
                TrayEvent::ShowWindow => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::SetPowerMode(mode) => {
                    let shared = Arc::clone(&self.state);
                    let mut state = self.state.lock().unwrap();
                    apply_power(&mut state, &shared, mode);
                }
                TrayEvent::Exit => {
                    exit_now(&self.state, &self.stop);
                }
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            let close_to_tray = self
                .state
                .lock()
                .unwrap()
                .session
                .config
                .lock()
                .unwrap()
                .close_to_tray;
            if close_to_tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                exit_now(&self.state, &self.stop);
            }
        }

        let mut content_height = 0.0;
        let shared = Arc::clone(&self.state);
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut state = self.state.lock().unwrap();
            load_icons(&mut state, ctx);
            if let (Some(_), Some(timestamp)) = (&state.error, state.error_at) {
                if timestamp.elapsed() > Duration::from_secs(5) {
                    state.error = None;
                    state.error_at = None;
                }
            }

            let lang = state.session.config.lock().unwrap().language();

            let start_y = ui.cursor().top();
            if let Some(version) = &state.firmware {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "ec_firmware"));
                    ui.label(version);
                });
            }
            ui.horizontal(|ui| {
                ui.label(t(lang, "secure_boot"));
                ui.label(&state.session.runtime.secure_boot);
            });
            ui.separator();

            if let Some(error) = &state.error {
                ui.colored_label(egui::Color32::RED, format!("{}{error}", t(lang, "error")));
                ui.separator();
            }

            if let Some(metrics) = state.metrics.clone() {
                draw_apu(ui, lang, &metrics, &mut state, &shared);
                ui.separator();
                for (index, key) in ["fan1", "fan2", "fan3"].into_iter().enumerate() {
                    draw_fan(ui, lang, t(lang, key), index, &metrics, &mut state, &shared);
                }
            } else {
                ui.label(t(lang, "loading"));
            }

            ui.separator();
            draw_settings(ui, lang, ctx, &mut state);
            content_height = ui.cursor().top() - start_y + 15.0;
        });

        resize_window(self, ctx, content_height);
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn draw_apu(
    ui: &mut egui::Ui,
    lang: Language,
    metrics: &MetricsSnapshot,
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
) {
    let response = ui.group(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("APU");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state.edit.apu_applying {
                        ui.add(egui::Spinner::new());
                    } else if (state.edit.apu_edit || !state.edit.other_edit_active(None))
                        && icon_button(ui, state, state.edit.apu_edit)
                    {
                        if state.edit.apu_edit {
                            apply_power(state, shared, state.edit.temp_power_mode.clone());
                        } else {
                            state.edit.apu_edit = true;
                            state.edit.temp_power_mode = metrics.power_mode.clone();
                        }
                    }
                });
            });

            if state.edit.apu_edit {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "power_mode"));
                    egui::ComboBox::from_id_salt("apu-mode")
                        .selected_text(t(lang, &state.edit.temp_power_mode))
                        .show_ui(ui, |ui| {
                            for mode in ["quiet", "balanced", "performance"] {
                                ui.selectable_value(
                                    &mut state.edit.temp_power_mode,
                                    mode.to_string(),
                                    t(lang, mode),
                                );
                            }
                        });
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "temperature"));
                    ui.colored_label(
                        temp_color(metrics.temperature),
                        format!("{}°C", metrics.temperature),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(t(lang, "current_mode"));
                    ui.colored_label(
                        power_color(&metrics.power_mode),
                        t(lang, &metrics.power_mode),
                    );
                });
                ui.label(t(lang, "power_mode"));
                ui.horizontal(|ui| {
                    for id in ["quiet", "balanced", "performance"] {
                        let selected = metrics.power_mode == id;
                        if ui.selectable_label(selected, t(lang, id)).clicked()
                            && !selected
                            && !state.edit.apu_applying
                        {
                            apply_power(state, shared, id.to_string());
                        }
                    }
                });
            }
        });
    });

    if !state.edit.apu_edit {
        let mut rect = response.response.rect;
        rect.set_width(ui.available_width());
        draw_chart(
            ui,
            rect,
            &state.charts.temperature,
            100,
            temp_color(metrics.temperature),
        );
    }
}

fn draw_fan(
    ui: &mut egui::Ui,
    lang: Language,
    name: &str,
    index: usize,
    metrics: &MetricsSnapshot,
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
) {
    let fan = &metrics.fans[index];
    let history = state.charts.fans[index].clone();
    let max_rpm = if index == 2 { 2500 } else { 5000 };
    let editing = state.edit.fan_edit[index];

    let response = ui.group(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading(name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state.edit.fan_applying[index] {
                        ui.add(egui::Spinner::new());
                    } else if (editing || !state.edit.other_edit_active(Some(index)))
                        && icon_button(ui, state, editing)
                    {
                        if editing {
                            apply_fan(state, shared, index);
                        } else {
                            state.edit.fan_edit[index] = true;
                            state.edit.temp_fan_mode[index] = fan.mode.clone();
                            state.edit.temp_fan_level[index] = i32::from(fan.level);
                            state.edit.temp_fan_rampup[index] = join_curve(&fan.rampup_curve);
                            state.edit.temp_fan_rampdown[index] = join_curve(&fan.rampdown_curve);
                        }
                    }
                });
            });

            if editing {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "mode"));
                    egui::ComboBox::from_id_salt(format!("fan-mode-{index}"))
                        .selected_text(t(lang, &state.edit.temp_fan_mode[index]))
                        .show_ui(ui, |ui| {
                            for mode in ["auto", "fixed", "curve"] {
                                ui.selectable_value(
                                    &mut state.edit.temp_fan_mode[index],
                                    mode.to_string(),
                                    t(lang, mode),
                                );
                            }
                        });
                });
                if state.edit.temp_fan_mode[index] == "fixed" {
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "level"));
                        ui.add(egui::Slider::new(
                            &mut state.edit.temp_fan_level[index],
                            0..=5,
                        ));
                    });
                }
                if state.edit.temp_fan_mode[index] == "curve" {
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "ramp_up"));
                        ui.text_edit_singleline(&mut state.edit.temp_fan_rampup[index]);
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "ramp_down"));
                        ui.text_edit_singleline(&mut state.edit.temp_fan_rampdown[index]);
                    });
                    ui.label(egui::RichText::new(t(lang, "hint_curve")).weak());
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "mode"));
                    ui.colored_label(mode_color(&fan.mode), t(lang, &fan.mode));
                });
                ui.horizontal(|ui| {
                    ui.label(t(lang, "rpm"));
                    ui.colored_label(rpm_color(fan.rpm), format!("{}", fan.rpm));
                });
                if fan.mode == "fixed" || fan.mode == "curve" {
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "level"));
                        ui.label(format!("{}", fan.level));
                    });
                }
                if fan.mode == "curve" {
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "ramp_up"));
                        ui.label(format!("{:?}", fan.rampup_curve));
                    });
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "ramp_down"));
                        ui.label(format!("{:?}", fan.rampdown_curve));
                    });
                }
            }
        });
    });

    if !editing {
        let mut rect = response.response.rect;
        rect.set_width(ui.available_width());
        draw_chart(ui, rect, &history, max_rpm, rpm_color(fan.rpm));
    }
}

fn draw_settings(ui: &mut egui::Ui, lang: Language, ctx: &egui::Context, state: &mut UiState) {
    let mut close_to_tray;
    let mut start_with_windows;
    let mut language;
    {
        let config = state.session.config.lock().unwrap();
        close_to_tray = config.close_to_tray;
        start_with_windows = config.start_with_windows;
        language = config.language.clone();
    }

    ui.group(|ui| {
        ui.heading(t(lang, "settings"));
        ui.horizontal(|ui| {
            ui.label(t(lang, "close_window"));
            if ui
                .radio_value(&mut close_to_tray, true, t(lang, "close_to_tray"))
                .changed()
            {
                persist_settings(state, close_to_tray, start_with_windows, language.clone());
            }
            if ui
                .radio_value(&mut close_to_tray, false, t(lang, "close_quit"))
                .changed()
            {
                persist_settings(state, close_to_tray, start_with_windows, language.clone());
            }
        });
        if ui
            .checkbox(&mut start_with_windows, t(lang, "start_with_windows"))
            .changed()
        {
            persist_settings(state, close_to_tray, start_with_windows, language.clone());
        }
        ui.horizontal(|ui| {
            ui.label(t(lang, "language"));
            let previous = language.clone();
            egui::ComboBox::from_id_salt("ui-language")
                .selected_text(if language == "en" {
                    t(lang, "lang_en")
                } else {
                    t(lang, "lang_zh")
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut language,
                        Language::Zh.code().to_string(),
                        t(lang, "lang_zh"),
                    );
                    ui.selectable_value(
                        &mut language,
                        Language::En.code().to_string(),
                        t(lang, "lang_en"),
                    );
                });
            if language != previous {
                persist_settings(state, close_to_tray, start_with_windows, language.clone());
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    t(Language::from_code(&language), "app_title").to_string(),
                ));
            }
        });
    });
}

fn persist_settings(
    state: &mut UiState,
    close_to_tray: bool,
    start_with_windows: bool,
    language: String,
) {
    let start_changed;
    let result = {
        let mut config = state.session.config.lock().unwrap();
        start_changed = config.start_with_windows != start_with_windows;
        config.close_to_tray = close_to_tray;
        config.start_with_windows = start_with_windows;
        config.language = language;
        config.save()
    };
    if let Err(error) = result {
        state.error = Some(error);
        state.error_at = Some(Instant::now());
        return;
    }
    if start_changed {
        if let Err(error) = set_start_with_windows(start_with_windows) {
            state.error = Some(error);
            state.error_at = Some(Instant::now());
        }
    }
}

fn exit_now(state: &Arc<Mutex<UiState>>, stop: &Arc<AtomicBool>) {
    stop.store(true, Ordering::SeqCst);
    let session = Arc::clone(&state.lock().unwrap().session);
    crate::tray::shutdown();
    session.shutdown();
    std::process::exit(0);
}

fn apply_power(state: &mut UiState, shared: &Arc<Mutex<UiState>>, mode: String) {
    state.edit.apu_applying = true;
    let session = Arc::clone(&state.session);
    let shared = Arc::clone(shared);
    thread::spawn(move || {
        let result = session
            .set_power_mode(&mode)
            .and_then(|_| session.metrics());
        let mut ui = shared.lock().unwrap();
        match result {
            Ok(metrics) => {
                ui.metrics = Some(metrics);
                ui.edit.apu_edit = false;
            }
            Err(error) => {
                ui.error = Some(error);
                ui.error_at = Some(Instant::now());
            }
        }
        ui.edit.apu_applying = false;
    });
}

fn apply_fan(state: &mut UiState, shared: &Arc<Mutex<UiState>>, index: usize) {
    state.edit.fan_applying[index] = true;
    let mode = state.edit.temp_fan_mode[index].clone();
    let level = state.edit.temp_fan_level[index] as u8;
    let rampup = parse_curve(&state.edit.temp_fan_rampup[index]);
    let rampdown = parse_curve(&state.edit.temp_fan_rampdown[index]);
    let session = Arc::clone(&state.session);
    let shared = Arc::clone(shared);
    thread::spawn(move || {
        let result = (|| {
            if mode == "curve" {
                let lang = session.config.lock().unwrap().language();
                let rampup = rampup.ok_or_else(|| t(lang, "error_rampup").to_string())?;
                let rampdown = rampdown.ok_or_else(|| t(lang, "error_rampdown").to_string())?;
                session.apply_fan_edit(
                    (index + 1) as u8,
                    &mode,
                    level,
                    Some(rampup),
                    Some(rampdown),
                )?;
            } else {
                session.apply_fan_edit((index + 1) as u8, &mode, level, None, None)?;
            }
            session.metrics()
        })();
        let mut ui = shared.lock().unwrap();
        match result {
            Ok(metrics) => {
                ui.metrics = Some(metrics);
                ui.edit.fan_edit[index] = false;
            }
            Err(error) => {
                ui.error = Some(error);
                ui.error_at = Some(Instant::now());
            }
        }
        ui.edit.fan_applying[index] = false;
    });
}

fn icon_button(ui: &mut egui::Ui, state: &UiState, editing: bool) -> bool {
    let texture = if editing { &state.check } else { &state.cog };
    let Some(texture) = texture else {
        return false;
    };
    let image = egui::Image::from_texture(texture).fit_to_exact_size(egui::Vec2::new(16.0, 16.0));
    ui.add(egui::Button::image(image).frame(false)).clicked()
}

fn load_icons(state: &mut UiState, ctx: &egui::Context) {
    if state.cog.is_none() {
        state.cog = texture_from_bytes(ctx, "cog", COG_ICON_BYTES);
    }
    if state.check.is_none() {
        state.check = texture_from_bytes(ctx, "check", CHECK_ICON_BYTES);
    }
}

fn texture_from_bytes(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = img.dimensions();
    Some(ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba),
        egui::TextureOptions::default(),
    ))
}

fn draw_chart(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    history: &VecDeque<i32>,
    max_value: i32,
    color: egui::Color32,
) {
    if history.is_empty() {
        return;
    }
    let painter = ui.painter();
    let bar_width = rect.width() / CHART_HISTORY_SIZE as f32;
    let max_bars = (rect.width() / bar_width).floor() as usize;
    let start_index = history.len().saturating_sub(max_bars);
    for (i, &value) in history.iter().skip(start_index).enumerate() {
        let height = rect.height() * (value as f32 / max_value as f32).clamp(0.0, 1.0);
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + i as f32 * bar_width, rect.max.y - height),
            egui::pos2(rect.min.x + (i as f32 + 1.0) * bar_width, rect.max.y),
        );
        painter.rect_filled(
            bar,
            0.0,
            egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40),
        );
    }
}

fn resize_window(app: &mut ControlApp, ctx: &egui::Context, content_height: f32) {
    let target = content_height.clamp(200.0, 800.0);
    if app.window_configured && (target - app.last_height).abs() <= 5.0 {
        return;
    }
    let size = egui::Vec2::new(400.0, target);
    #[cfg(windows)]
    let screen = unsafe {
        use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        [
            GetSystemMetrics(SM_CXSCREEN) as f32,
            GetSystemMetrics(SM_CYSCREEN) as f32,
        ]
    };
    #[cfg(not(windows))]
    let screen = [1920.0, 1080.0];
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
    if !app.window_configured {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            (screen[0] - size.x) / 2.0,
            (screen[1] - size.y) / 2.0,
        )));
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
    app.last_height = target;
    app.window_configured = true;
}

fn parse_curve(text: &str) -> Option<[u8; 5]> {
    let values: Vec<u8> = text
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    values.try_into().ok()
}

fn join_curve(curve: &[u8; 5]) -> String {
    curve
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn temp_color(temp: u8) -> egui::Color32 {
    if temp <= 50 {
        egui::Color32::GREEN
    } else if temp <= 85 {
        egui::Color32::YELLOW
    } else {
        egui::Color32::RED
    }
}

fn rpm_color(rpm: u16) -> egui::Color32 {
    if rpm <= 1250 {
        egui::Color32::GREEN
    } else if rpm <= 2500 {
        egui::Color32::YELLOW
    } else {
        egui::Color32::RED
    }
}

fn mode_color(mode: &str) -> egui::Color32 {
    match mode {
        "auto" => egui::Color32::MAGENTA,
        "fixed" => egui::Color32::GRAY,
        "curve" => egui::Color32::from_rgb(0, 255, 255),
        _ => egui::Color32::WHITE,
    }
}

fn power_color(mode: &str) -> egui::Color32 {
    match mode {
        "quiet" => egui::Color32::GREEN,
        "balanced" => egui::Color32::LIGHT_BLUE,
        "performance" => egui::Color32::RED,
        _ => egui::Color32::WHITE,
    }
}
