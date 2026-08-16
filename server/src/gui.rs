use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Shadow, Stroke,
};
use image::GenericImageView;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::alert::{should_fire_temp_alert, TEMP_ALERT_MAX, TEMP_ALERT_MIN};
use crate::config::{
    log_file_path, program_data_dir, FanConfig, PortableConfig, SMOOTHING_WINDOW_MAX,
    SMOOTHING_WINDOW_MIN,
};
use crate::curve::{clamp_rampup_point, derive_rampdown, CURVE_TEMP_MAX};
use crate::diagnose::{DiagnoseReport, AUTHOR, REPO_URL, UPSTREAM_URL};
use crate::fan;
use crate::i18n::{t, Language};
use crate::platform::{
    host_name, pick_json_file, set_start_with_windows, shell_open, ui_chrome, UiChrome,
};
use crate::session::{AppSession, MetricsSnapshot};
use crate::thermal::TemperatureSource;
use crate::tray::TrayEvent;

const ICON_BYTES: &[u8] = include_bytes!("../assets/ec-su_axb35-win.png");
const CHART_HISTORY_SIZE: usize = 60;
const WINDOW_WIDTH: f32 = 480.0 * 0.6 * 1.4;
const WINDOW_HEIGHT: f32 = 640.0 * 0.6 * 1.4;
const WINDOW_MIN_HEIGHT: f32 = 240.0 * 1.4;
const CARD_RADIUS: f32 = 8.0;
const CONTROL_RADIUS: f32 = 4.0;
const ICON_SETTING: &str = "\u{E713}";
const ICON_BACK: &str = "\u{E72B}";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Settings,
    About,
    Diagnostics,
}

#[derive(Clone)]
struct Nav {
    page: Page,
    stack: Vec<Page>,
}

impl Nav {
    fn home() -> Self {
        Self {
            page: Page::Home,
            stack: Vec::new(),
        }
    }

    fn go(&mut self, dest: Page) {
        if dest == self.page {
            return;
        }
        self.stack.push(self.page);
        self.page = dest;
    }

    fn back(&mut self) {
        self.page = self.stack.pop().unwrap_or(Page::Home);
    }
}

#[derive(Clone, Copy)]
struct Theme {
    dark: bool,
    bg: Color32,
    surface: Color32,
    text: Color32,
    muted: Color32,
    accent: Color32,
    accent_fg: Color32,
    control: Color32,
    control_hover: Color32,
    stroke: Color32,
    ok: Color32,
    warn: Color32,
    bad: Color32,
    shadow: Color32,
}

impl Theme {
    fn from_chrome(chrome: UiChrome) -> Self {
        let accent = Color32::from_rgb(chrome.accent.0, chrome.accent.1, chrome.accent.2);
        let accent_fg = if (u16::from(chrome.accent.0)
            + u16::from(chrome.accent.1)
            + u16::from(chrome.accent.2))
            >= 500
        {
            Color32::from_rgb(26, 26, 26)
        } else {
            Color32::WHITE
        };
        if chrome.dark {
            Self {
                dark: true,
                bg: Color32::from_rgb(32, 32, 32),
                surface: Color32::from_rgb(44, 44, 44),
                text: Color32::from_rgb(255, 255, 255),
                muted: Color32::from_rgb(197, 197, 197),
                accent,
                accent_fg,
                control: Color32::from_rgb(55, 55, 55),
                control_hover: Color32::from_rgb(68, 68, 68),
                stroke: Color32::from_white_alpha(20),
                ok: Color32::from_rgb(108, 203, 95),
                warn: Color32::from_rgb(252, 225, 0),
                bad: Color32::from_rgb(255, 153, 164),
                shadow: Color32::from_white_alpha(20),
            }
        } else {
            Self {
                dark: false,
                bg: Color32::from_rgb(243, 243, 243),
                surface: Color32::WHITE,
                text: Color32::from_rgb(26, 26, 26),
                muted: Color32::from_rgb(92, 92, 92),
                accent,
                accent_fg,
                control: Color32::from_rgb(251, 251, 251),
                control_hover: Color32::from_rgb(237, 237, 237),
                stroke: Color32::from_black_alpha(15),
                ok: Color32::from_rgb(16, 124, 16),
                warn: Color32::from_rgb(157, 93, 0),
                bad: Color32::from_rgb(196, 43, 28),
                shadow: Color32::from_black_alpha(15),
            }
        }
    }

    fn icons() -> FontFamily {
        FontFamily::Name("icons".into())
    }
}

#[derive(Clone, Debug)]
struct EditState {
    apu_applying: bool,
    fan_applying: [bool; 3],
    temp_fan_level: [i32; 3],
    dragging_level: [bool; 3],
    temp_fan_rampup: [[u8; 5]; 3],
    temp_fan_rampdown: [[u8; 5]; 3],
    dragging_curve: [bool; 3],
    curve_dirty: [bool; 3],
    renaming_fan: [bool; 3],
    temp_fan_name: [String; 3],
    renaming_processor: bool,
    temp_processor_name: String,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            apu_applying: false,
            fan_applying: [false; 3],
            temp_fan_level: [0, 0, 0],
            dragging_level: [false; 3],
            temp_fan_rampup: [
                [60, 70, 83, 95, 97],
                [60, 70, 83, 95, 97],
                [20, 60, 83, 95, 97],
            ],
            temp_fan_rampdown: [
                [40, 50, 80, 94, 96],
                [40, 50, 80, 94, 96],
                [0, 50, 80, 94, 96],
            ],
            dragging_curve: [false; 3],
            curve_dirty: [false; 3],
            renaming_fan: [false; 3],
            temp_fan_name: [String::new(), String::new(), String::new()],
            renaming_processor: false,
            temp_processor_name: String::new(),
        }
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
    diagnose: Option<DiagnoseReport>,
}

struct ControlApp {
    state: Arc<Mutex<UiState>>,
    stop: Arc<AtomicBool>,
    window_configured: bool,
    last_chrome: Option<UiChrome>,
    tray_rx: Receiver<TrayEvent>,
    nav: Nav,
    last_temp_alert: Option<Instant>,
    hidden_to_tray: bool,
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
        diagnose: None,
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
        last_chrome: None,
        tray_rx,
        nav: Nav::home(),
        last_temp_alert: None,
        hidden_to_tray: false,
    };
    let title = t(session.config.lock().unwrap().language(), "app_title");

    eframe::run_native(
        title,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
                .with_min_inner_size([WINDOW_WIDTH, WINDOW_MIN_HEIGHT])
                .with_maximize_button(false)
                .with_icon(icon.unwrap_or_default()),
            ..Default::default()
        },
        Box::new(move |cc| {
            install_windows_fonts(&cc.egui_ctx);
            crate::tray::bind_gui(cc.egui_ctx.clone(), hwnd_from_handle(&cc.window_handle()));
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| format!("Failed to start GUI: {error}"))
}

fn hwnd_from_handle(
    handle: &Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>,
) -> isize {
    handle
        .as_ref()
        .ok()
        .and_then(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(win) => Some(win.hwnd.get()),
            _ => None,
        })
        .unwrap_or(0)
}

fn install_theme(ctx: &egui::Context, theme: &Theme) {
    let mut visuals = if theme.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = theme.dark;
    visuals.override_text_color = Some(theme.text);
    visuals.weak_text_color = Some(theme.muted);
    visuals.window_fill = theme.bg;
    visuals.panel_fill = theme.bg;
    visuals.faint_bg_color = theme.control;
    visuals.extreme_bg_color = theme.control;
    visuals.code_bg_color = theme.control;
    visuals.hyperlink_color = theme.accent;
    visuals.warn_fg_color = theme.warn;
    visuals.error_fg_color = theme.bad;
    visuals.window_stroke = Stroke::new(1.0, theme.stroke);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.selection.bg_fill = theme.accent;
    visuals.selection.stroke = Stroke::new(1.0, theme.accent_fg);
    let control = WidgetFill {
        fill: theme.control,
        hover: theme.control_hover,
        stroke: theme.stroke,
        text: theme.text,
        muted: theme.muted,
    };
    apply_widget_visuals(&mut visuals, control);

    let mut style = (*ctx.global_style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = Margin::same(12);
    style.spacing.interact_size = egui::vec2(32.0, 28.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    ctx.set_global_style(style);
}

struct WidgetFill {
    fill: Color32,
    hover: Color32,
    stroke: Color32,
    text: Color32,
    muted: Color32,
}

fn apply_widget_visuals(visuals: &mut egui::Visuals, colors: WidgetFill) {
    let radius = CornerRadius::same(CONTROL_RADIUS as u8);
    visuals.widgets.noninteractive.bg_fill = colors.fill;
    visuals.widgets.noninteractive.weak_bg_fill = colors.fill;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.stroke);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.muted);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.noninteractive.expansion = 0.0;
    visuals.widgets.inactive.bg_fill = colors.fill;
    visuals.widgets.inactive.weak_bg_fill = colors.fill;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, colors.stroke);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, colors.text);
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.inactive.expansion = 0.0;
    visuals.widgets.hovered.bg_fill = colors.hover;
    visuals.widgets.hovered.weak_bg_fill = colors.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, colors.stroke);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, colors.text);
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.hovered.expansion = 0.0;
    visuals.widgets.active.bg_fill = colors.hover;
    visuals.widgets.active.weak_bg_fill = colors.hover;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, colors.stroke);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, colors.text);
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.active.expansion = 0.0;
    visuals.widgets.open.bg_fill = colors.fill;
    visuals.widgets.open.weak_bg_fill = colors.fill;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, colors.stroke);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, colors.text);
    visuals.widgets.open.corner_radius = radius;
    visuals.widgets.open.expansion = 0.0;
}

fn install_windows_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = load_font_bytes(&["segoeui.ttf", "SegUIVar.ttf", "segoeuisl.ttf"]) {
        fonts.font_data.insert(
            "segoe".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "segoe".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, "segoe".to_owned());
        }
    }
    if let Some(bytes) = load_font_bytes(&["consola.ttf", "CascadiaMono.ttf"]) {
        fonts.font_data.insert(
            "consolas".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, "consolas".to_owned());
        }
    }
    if let Some(bytes) = load_cjk_font_bytes() {
        fonts.font_data.insert(
            "cjk".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.push("cjk".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push("cjk".to_owned());
        }
    }
    let mut icon_fonts = Vec::new();
    if let Some(bytes) = load_font_bytes(&["SegoeIcons.ttf", "segmdl2.ttf"]) {
        fonts.font_data.insert(
            "icons".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        icon_fonts.push("icons".to_owned());
    }
    if fonts.font_data.contains_key("segoe") {
        icon_fonts.push("segoe".to_owned());
    }
    if let Some(proportional) = fonts.families.get(&FontFamily::Proportional) {
        for name in proportional {
            if !icon_fonts.contains(name) {
                icon_fonts.push(name.clone());
            }
        }
    }
    fonts.families.insert(Theme::icons(), icon_fonts);
    ctx.set_fonts(fonts);
}

fn load_font_bytes(names: &[&str]) -> Option<Vec<u8>> {
    let fonts = std::path::Path::new(&crate::harden::windows_directory()).join("Fonts");
    for name in names {
        if let Ok(bytes) = std::fs::read(fonts.join(name)) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    None
}

fn load_cjk_font_bytes() -> Option<Vec<u8>> {
    let fonts = std::path::Path::new(&crate::harden::windows_directory()).join("Fonts");
    for name in [
        "msyh.ttc",
        "msyh.ttf",
        "msjh.ttc",
        "simhei.ttf",
        "Deng.ttf",
        "simsun.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(fonts.join(name)) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    None
}

pub(crate) fn hide_on_close(close_to_tray: bool, quitting: bool) -> bool {
    close_to_tray && !quitting
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IdlePaintPolicy {
    paint_ui: bool,
    repaint_after: Duration,
}

fn idle_paint_policy(hidden_to_tray: bool) -> IdlePaintPolicy {
    if hidden_to_tray {
        IdlePaintPolicy {
            paint_ui: false,
            repaint_after: Duration::from_secs(1),
        }
    } else {
        IdlePaintPolicy {
            paint_ui: true,
            repaint_after: Duration::from_millis(200),
        }
    }
}

impl eframe::App for ControlApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        crate::tray::set_gui_hwnd(hwnd_from_handle(&frame.window_handle()));

        while let Ok(event) = self.tray_rx.try_recv() {
            match event {
                TrayEvent::ShowWindow => {
                    self.hidden_to_tray = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::PowerModeChanged => {
                    refresh_metrics(&self.state);
                }
                TrayEvent::Exit => {
                    exit_now(&self.state, &self.stop);
                }
            }
        }

        if crate::tray::is_quitting() {
            exit_now(&self.state, &self.stop);
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
            if hide_on_close(close_to_tray, crate::tray::is_quitting()) {
                self.hidden_to_tray = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                exit_now(&self.state, &self.stop);
            }
        }

        if self.state.lock().unwrap().session.poll_reload_fans() {
            let session = Arc::clone(&self.state.lock().unwrap().session);
            session.adopt_saved_fan_curves();
            refresh_metrics(&self.state);
        }

        maybe_temp_alert(self);
        ctx.request_repaint_after(idle_paint_policy(self.hidden_to_tray).repaint_after);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !idle_paint_policy(self.hidden_to_tray).paint_ui {
            return;
        }

        let ctx = ui.ctx().clone();
        let chrome = ui_chrome();
        if self.last_chrome != Some(chrome) {
            let theme = Theme::from_chrome(chrome);
            install_theme(&ctx, &theme);
            self.last_chrome = Some(chrome);
        }
        let theme = Theme::from_chrome(chrome);

        if self.nav.page != Page::Home && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.nav.back();
        }

        let shared = Arc::clone(&self.state);
        let hwnd = hwnd_from_handle(&frame.window_handle());
        let mut nav = self.nav.clone();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme.bg)
                    .inner_margin(Margin::same(10)),
            )
            .show_inside(ui, |ui| {
                let mut state = self.state.lock().unwrap();
                if let (Some(_), Some(timestamp)) = (&state.error, state.error_at) {
                    if timestamp.elapsed() > Duration::from_secs(5) {
                        state.error = None;
                        state.error_at = None;
                    }
                }
                if nav.page != Page::Diagnostics {
                    state.diagnose = None;
                }

                let lang = state.session.config.lock().unwrap().language();
                let page = nav.page;
                let scroll_id = match page {
                    Page::Home => "home-scroll",
                    Page::Settings => "settings-scroll",
                    Page::About => "about-scroll",
                    Page::Diagnostics => "diagnostics-scroll",
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt(scroll_id)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match page {
                            Page::Home => {
                                draw_header(ui, lang, &theme, &state, &mut nav);
                                draw_error(ui, lang, &theme, &state);
                                ui.add_space(10.0);
                                if let Some(metrics) = state.metrics.clone() {
                                    draw_apu(ui, lang, &theme, &metrics, &mut state, &shared);
                                    for index in 0..metrics.fans.len() {
                                        ui.add_space(10.0);
                                        draw_fan(
                                            ui, lang, &theme, index, &metrics, &mut state, &shared,
                                        );
                                    }
                                } else {
                                    ui.label(RichText::new(t(lang, "loading")).color(theme.muted));
                                }
                            }
                            Page::Settings => {
                                draw_page_header(ui, lang, &theme, &mut nav, "settings");
                                draw_error(ui, lang, &theme, &state);
                                ui.add_space(10.0);
                                draw_settings(
                                    ui, lang, &theme, &ctx, &mut state, &shared, &mut nav, hwnd,
                                );
                            }
                            Page::About => {
                                draw_page_header(ui, lang, &theme, &mut nav, "about");
                                draw_error(ui, lang, &theme, &state);
                                ui.add_space(10.0);
                                draw_about(ui, lang, &theme, &state, &mut nav);
                            }
                            Page::Diagnostics => {
                                draw_page_header(ui, lang, &theme, &mut nav, "diagnostics");
                                draw_error(ui, lang, &theme, &state);
                                ui.add_space(10.0);
                                draw_diagnostics(ui, lang, &theme, &ctx, &mut state);
                            }
                        }
                    });
            });
        self.nav = nav;

        configure_window(self, &ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop.store(true, Ordering::SeqCst);
        let session = Arc::clone(&self.state.lock().unwrap().session);
        session.shutdown();
    }
}

fn card(theme: &Theme) -> egui::Frame {
    let mut frame = egui::Frame::NONE
        .fill(theme.surface)
        .corner_radius(CARD_RADIUS)
        .inner_margin(Margin::same(10));
    if theme.dark {
        frame = frame
            .stroke(Stroke::new(1.0, theme.stroke))
            .shadow(Shadow::NONE);
    } else {
        frame = frame.stroke(Stroke::new(1.0, theme.stroke)).shadow(Shadow {
            offset: [0, 1],
            blur: 2,
            spread: 0,
            color: theme.shadow,
        });
    }
    frame
}

fn draw_error(ui: &mut egui::Ui, lang: Language, theme: &Theme, state: &UiState) {
    if let Some(error) = &state.error {
        ui.add_space(8.0);
        card(theme)
            .stroke(Stroke::new(1.0, theme.bad))
            .show(ui, |ui| {
                ui.colored_label(theme.bad, format!("{}{error}", t(lang, "error")));
            });
    }
}

fn draw_header(ui: &mut egui::Ui, lang: Language, theme: &Theme, state: &UiState, nav: &mut Nav) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t(lang, "app_title"))
                .size(16.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if nav_icon(ui, theme, ICON_SETTING, t(lang, "settings")) {
                nav.go(Page::Settings);
            }
        });
    });
    ui.add_space(4.0);
    let mut caption = String::new();
    if let Some(version) = &state.firmware {
        caption.push_str(&format!("{}{}", t(lang, "ec_firmware"), version));
        caption.push_str("  \u{00B7}  ");
    }
    caption.push_str(&format!(
        "{}{}",
        t(lang, "secure_boot"),
        state.session.runtime.secure_boot
    ));
    ui.add(egui::Label::new(RichText::new(caption).small().color(theme.muted)).wrap());
}

fn draw_page_header(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    nav: &mut Nav,
    title_key: &str,
) {
    ui.horizontal(|ui| {
        if nav_icon(ui, theme, ICON_BACK, t(lang, "back")) {
            nav.back();
        }
        ui.label(
            RichText::new(t(lang, title_key))
                .size(16.0)
                .strong()
                .color(theme.text),
        );
    });
}

fn draw_apu(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    metrics: &MetricsSnapshot,
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
) {
    let default_name = host_name();
    let custom_name = state.session.config.lock().unwrap().processor_custom_name();
    let name = fan::display_name(custom_name.as_deref(), &default_name);
    let has_custom = custom_name.is_some();

    card(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            if state.edit.renaming_processor {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.edit.temp_processor_name)
                        .desired_width((ui.available_width() - 108.0).max(72.0))
                        .hint_text(&default_name),
                );
                if !response.has_focus() && !response.lost_focus() {
                    response.request_focus();
                }
                let restore = ui.small_button(t(lang, "restore_default")).clicked();
                let enter =
                    response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
                if restore {
                    persist_processor_name(state, None);
                    state.edit.renaming_processor = false;
                } else if escape {
                    state.edit.renaming_processor = false;
                } else if enter || response.lost_focus() {
                    persist_processor_name(state, Some(state.edit.temp_processor_name.clone()));
                    state.edit.renaming_processor = false;
                }
            } else {
                ui.heading(&name);
                if state.edit.apu_applying {
                    ui.add(egui::Spinner::new().size(14.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if has_custom && ui.small_button(t(lang, "restore_default")).clicked() {
                        persist_processor_name(state, None);
                    }
                    if ui.small_button(t(lang, "rename")).clicked() {
                        state.edit.temp_processor_name = name.clone();
                        state.edit.renaming_processor = true;
                    }
                });
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{}\u{00B0}C", metrics.temperature))
                    .size(20.0)
                    .strong()
                    .color(temp_color(theme, metrics.temperature))
                    .family(FontFamily::Monospace),
            );
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(t(lang, metrics.temperature_source.i18n_key()))
                        .small()
                        .color(theme.muted),
                );
                ui.label(
                    RichText::new(t(lang, &metrics.power_mode))
                        .small()
                        .color(theme.muted),
                );
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            sensor_chip(ui, theme, "GPU", metrics.gpu_temp);
            sensor_chip(ui, theme, "CPU", metrics.cpu_temp);
            sensor_chip(ui, theme, "SoC", metrics.soc_temp);
            sensor_chip(ui, theme, "Hotspot", metrics.hotspot_temp);
            sensor_chip(ui, theme, "EC", Some(metrics.ec_raw_temp));
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(t(lang, "power_mode"))
                .small()
                .color(theme.muted),
        );
        ui.add_space(4.0);
        ui.add_enabled_ui(!state.edit.apu_applying, |ui| {
            segmented(ui, theme, 3, |ui, width| {
                for id in ["quiet", "balanced", "performance"] {
                    if mode_button(ui, t(lang, id), metrics.power_mode == id, width)
                        && metrics.power_mode != id
                    {
                        apply_power(state, shared, id.to_string());
                    }
                }
            });
        });
        ui.add_space(8.0);
        let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 28.0));
        draw_chart(
            ui,
            theme,
            rect,
            &state.charts.temperature,
            100,
            temp_color(theme, metrics.temperature),
        );
    });
}

fn draw_fan(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    index: usize,
    metrics: &MetricsSnapshot,
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
) {
    let default_name = t(lang, fan::title_key((index + 1) as u8));
    let custom_name = state
        .session
        .config
        .lock()
        .unwrap()
        .fan_custom_name((index + 1) as u8);
    let name = fan::display_name(custom_name.as_deref(), default_name);
    let has_custom = custom_name.is_some();
    let fan = &metrics.fans[index];
    let history = state.charts.fans[index].clone();
    let max_rpm = if index == 2 { 2500 } else { 5000 };
    if !state.edit.dragging_level[index] {
        state.edit.temp_fan_level[index] = i32::from(fan.level);
    }
    if !state.edit.dragging_curve[index]
        && !state.edit.fan_applying[index]
        && !state.edit.curve_dirty[index]
    {
        state.edit.temp_fan_rampup[index] = fan.rampup_curve;
        state.edit.temp_fan_rampdown[index] = fan.rampdown_curve;
    }
    let applying = state.edit.fan_applying[index];

    card(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            if state.edit.renaming_fan[index] {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.edit.temp_fan_name[index])
                        .desired_width((ui.available_width() - 108.0).max(72.0))
                        .hint_text(default_name),
                );
                if !response.has_focus() && !response.lost_focus() {
                    response.request_focus();
                }
                let restore = ui.small_button(t(lang, "restore_default")).clicked();
                let enter =
                    response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
                if restore {
                    persist_fan_name(state, index, None);
                    state.edit.renaming_fan[index] = false;
                } else if escape {
                    state.edit.renaming_fan[index] = false;
                } else if enter || response.lost_focus() {
                    persist_fan_name(state, index, Some(state.edit.temp_fan_name[index].clone()));
                    state.edit.renaming_fan[index] = false;
                }
            } else {
                ui.heading(&name);
                ui.label(RichText::new(t(lang, &fan.mode)).small().color(theme.muted));
                if applying {
                    ui.add(egui::Spinner::new().size(14.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if has_custom && ui.small_button(t(lang, "restore_default")).clicked() {
                        persist_fan_name(state, index, None);
                    }
                    if ui.small_button(t(lang, "rename")).clicked() {
                        state.edit.temp_fan_name[index] = name.clone();
                        state.edit.renaming_fan[index] = true;
                    }
                });
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{}", fan.rpm))
                    .size(18.0)
                    .strong()
                    .color(rpm_color(theme, fan.rpm))
                    .family(FontFamily::Monospace),
            );
            ui.label(RichText::new("RPM").color(theme.muted));
            if fan.mode == "fixed" || fan.mode == "curve" {
                ui.label(RichText::new("\u{00B7}").color(theme.muted));
                ui.label(
                    RichText::new(format!("{}{}", t(lang, "level"), fan.level)).color(theme.muted),
                );
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new(t(lang, "mode")).small().color(theme.muted));
        ui.add_space(4.0);
        ui.add_enabled_ui(!applying, |ui| {
            segmented(ui, theme, 3, |ui, width| {
                for id in ["auto", "fixed", "curve"] {
                    if mode_button(ui, t(lang, id), fan.mode == id, width) && fan.mode != id {
                        apply_fan(
                            state,
                            shared,
                            index,
                            id.to_string(),
                            state.edit.temp_fan_level[index] as u8,
                            Some(fan.rampup_curve),
                            Some(fan.rampdown_curve),
                        );
                    }
                }
            });
        });
        if fan.mode == "fixed" {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(t(lang, "level")).color(theme.muted));
                ui.add_enabled_ui(!applying, |ui| {
                    let response = ui.add(egui::Slider::new(
                        &mut state.edit.temp_fan_level[index],
                        0..=5,
                    ));
                    state.edit.dragging_level[index] = response.dragged();
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        apply_fan(
                            state,
                            shared,
                            index,
                            "fixed".into(),
                            state.edit.temp_fan_level[index] as u8,
                            None,
                            None,
                        );
                    }
                });
            });
        }
        if fan.mode == "curve" {
            ui.add_space(6.0);
            draw_curve_table(ui, lang, theme, index, fan.level, applying, state, shared);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t(lang, "hint_curve"))
                        .small()
                        .color(theme.muted),
                );
                let defaults = FanConfig::default_for_fan((index + 1) as u8);
                let is_default = state.edit.temp_fan_rampup[index] == defaults.rampup_curve
                    && state.edit.temp_fan_rampdown[index] == defaults.rampdown_curve;
                if !is_default {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(!applying, |ui| {
                            if ui.small_button(t(lang, "restore_default")).clicked() {
                                restore_fan_curve(state, shared, index, fan.level);
                            }
                        });
                    });
                }
            });
        }
        ui.add_space(8.0);
        let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 28.0));
        draw_chart(
            ui,
            theme,
            rect,
            &history,
            max_rpm,
            rpm_color(theme, fan.rpm),
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_settings(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    ctx: &egui::Context,
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
    nav: &mut Nav,
    hwnd: isize,
) {
    let mut close_to_tray;
    let mut start_with_windows;
    let mut language;
    let mut temperature_source;
    let mut temp_alert_enabled;
    let mut temp_alert_celsius;
    let mut smoothing_window;
    {
        let config = state.session.config.lock().unwrap();
        close_to_tray = config.close_to_tray;
        start_with_windows = config.start_with_windows;
        language = config.language.clone();
        temperature_source = config.temperature_source.clone();
        temp_alert_enabled = config.temp_alert_enabled;
        temp_alert_celsius = i32::from(config.temp_alert_celsius);
        smoothing_window = i32::from(config.smoothing_window);
    }

    card(theme).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(t(lang, "close_window")).color(theme.text));
        ui.add_space(4.0);
        segmented(ui, theme, 2, |ui, width| {
            if mode_button(ui, t(lang, "close_to_tray"), close_to_tray, width) && !close_to_tray {
                close_to_tray = true;
                persist_settings(state, close_to_tray, start_with_windows, language.clone());
            }
            if mode_button(ui, t(lang, "close_quit"), !close_to_tray, width) && close_to_tray {
                close_to_tray = false;
                persist_settings(state, close_to_tray, start_with_windows, language.clone());
            }
        });
        hairline(ui, theme);
        settings_row(ui, |ui| {
            ui.label(RichText::new(t(lang, "start_with_windows")).color(theme.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if toggle_switch(ui, theme, &mut start_with_windows).changed() {
                    persist_settings(state, close_to_tray, start_with_windows, language.clone());
                }
            });
        });
        hairline(ui, theme);
        settings_row(ui, |ui| {
            ui.label(RichText::new(t(lang, "language")).color(theme.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
        hairline(ui, theme);
        settings_row(ui, |ui| {
            ui.label(RichText::new(t(lang, "temp_source")).color(theme.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let previous = temperature_source.clone();
                let selected = TemperatureSource::from_code(&temperature_source);
                egui::ComboBox::from_id_salt("ui-temp-source")
                    .selected_text(t(lang, selected.i18n_key()))
                    .show_ui(ui, |ui| {
                        for source in TemperatureSource::ALL {
                            ui.selectable_value(
                                &mut temperature_source,
                                source.code().to_string(),
                                t(lang, source.i18n_key()),
                            );
                        }
                    });
                if temperature_source != previous {
                    persist_temp_source(state, temperature_source.clone());
                }
            });
        });
        ui.label(
            RichText::new(t(lang, "temp_source_hint"))
                .small()
                .color(theme.muted),
        );
        hairline(ui, theme);
        ui.label(RichText::new(t(lang, "smoothing_window")).color(theme.text));
        if ui
            .add(egui::Slider::new(
                &mut smoothing_window,
                i32::from(SMOOTHING_WINDOW_MIN)..=i32::from(SMOOTHING_WINDOW_MAX),
            ))
            .changed()
        {
            persist_smoothing(state, smoothing_window as u8);
        }
        ui.label(
            RichText::new(t(lang, "smoothing_window_hint"))
                .small()
                .color(theme.muted),
        );
        hairline(ui, theme);
        settings_row(ui, |ui| {
            ui.label(RichText::new(t(lang, "temp_alert")).color(theme.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if toggle_switch(ui, theme, &mut temp_alert_enabled).changed() {
                    persist_alert(state, temp_alert_enabled, temp_alert_celsius as u8);
                }
            });
        });
        if temp_alert_enabled {
            hairline(ui, theme);
            ui.label(RichText::new(t(lang, "temp_alert_threshold")).color(theme.muted));
            if ui
                .add(egui::Slider::new(
                    &mut temp_alert_celsius,
                    i32::from(TEMP_ALERT_MIN)..=i32::from(TEMP_ALERT_MAX),
                ))
                .changed()
            {
                persist_alert(state, temp_alert_enabled, temp_alert_celsius as u8);
            }
        }
    });

    ui.add_space(10.0);
    card(theme).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let width = settings_button_width(ui);
        ui.horizontal(|ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(32.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            if ui
                .add_sized([width, 28.0], egui::Button::new(t(lang, "export_config")))
                .clicked()
            {
                export_config(state, hwnd);
            }
            if ui
                .add_sized([width, 28.0], egui::Button::new(t(lang, "import_config")))
                .clicked()
            {
                import_config(state, shared, hwnd);
            }
        });
        hairline(ui, theme);
        ui.horizontal(|ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(32.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            if ui
                .add_sized([width, 28.0], egui::Button::new(t(lang, "open_log")))
                .clicked()
            {
                shell_open(&log_file_path());
            }
            if ui
                .add_sized([width, 28.0], egui::Button::new(t(lang, "open_log_dir")))
                .clicked()
            {
                shell_open(&program_data_dir());
            }
        });
        hairline(ui, theme);
        ui.horizontal(|ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(32.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            if ui
                .add_sized([width, 28.0], egui::Button::new(t(lang, "about")))
                .clicked()
            {
                nav.go(Page::About);
            }
            if ui
                .add_sized([width, 28.0], egui::Button::new(t(lang, "diagnostics")))
                .clicked()
            {
                nav.go(Page::Diagnostics);
            }
        });
    });

    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(format!(
                "{} {} · {}",
                t(lang, "app_name"),
                env!("CARGO_PKG_VERSION"),
                AUTHOR
            ))
            .small()
            .color(theme.muted),
        );
    });
}

fn draw_about(ui: &mut egui::Ui, lang: Language, theme: &Theme, state: &UiState, nav: &mut Nav) {
    card(theme).show(ui, |ui| {
        ui.label(
            RichText::new(t(lang, "app_name"))
                .size(16.0)
                .strong()
                .color(theme.text),
        );
        ui.add_space(8.0);
        kv_line(ui, theme, t(lang, "version"), env!("CARGO_PKG_VERSION"));
        kv_line(ui, theme, t(lang, "author"), AUTHOR);
        link_line(ui, theme, t(lang, "forked_from"), UPSTREAM_URL);
        link_line(ui, theme, t(lang, "repository"), REPO_URL);
        ui.add_space(8.0);
        kv_line(
            ui,
            theme,
            t(lang, "motherboard"),
            &state.session.runtime.hardware,
        );
        kv_line(
            ui,
            theme,
            t(lang, "firmware_status"),
            state.firmware.as_deref().unwrap_or(t(lang, "unknown")),
        );
        kv_line(
            ui,
            theme,
            t(lang, "pawnio_status"),
            &state.session.runtime.pawnio,
        );
        kv_line(
            ui,
            theme,
            t(lang, "lpcacpiec_status"),
            &state.session.runtime.lpcacpiec,
        );
        kv_line(
            ui,
            theme,
            t(lang, "secure_boot_status"),
            &state.session.runtime.secure_boot,
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(t(lang, "pawnio_lgpl_note"))
                .small()
                .color(theme.muted),
        );
        ui.add_space(8.0);
        if ui.button(t(lang, "diagnostics")).clicked() {
            nav.go(Page::Diagnostics);
        }
    });
}

fn draw_diagnostics(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    ctx: &egui::Context,
    state: &mut UiState,
) {
    if state.diagnose.is_none() {
        state.diagnose = Some(DiagnoseReport::collect(Some(&state.session)));
    }
    let text = state
        .diagnose
        .as_ref()
        .map(|report| report.format_text(lang))
        .unwrap_or_default();
    card(theme).show(ui, |ui| {
        ui.label(
            RichText::new(&text)
                .color(theme.text)
                .family(FontFamily::Monospace),
        );
        ui.add_space(8.0);
        settings_row(ui, |ui| {
            if ui.button(t(lang, "copy_diagnostics")).clicked() {
                ctx.copy_text(text.clone());
            }
            if ui.button(t(lang, "open_log")).clicked() {
                shell_open(&log_file_path());
            }
            if ui.button(t(lang, "open_log_dir")).clicked() {
                shell_open(&program_data_dir());
            }
        });
    });
}

fn kv_line(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme.muted));
        ui.label(RichText::new(value).color(theme.text));
    });
}

fn link_line(ui: &mut egui::Ui, theme: &Theme, label: &str, url: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(label).color(theme.muted));
        ui.hyperlink_to(url, url);
    });
}

fn persist_processor_name(state: &mut UiState, name: Option<String>) {
    let default_name = host_name();
    let stored = name
        .as_deref()
        .and_then(fan::sanitize_name)
        .filter(|value| value != &default_name);
    state.session.set_processor_name(stored);
}

fn persist_fan_name(state: &mut UiState, index: usize, name: Option<String>) {
    let fan_id = (index + 1) as u8;
    let default_name = {
        let lang = state.session.config.lock().unwrap().language();
        t(lang, fan::title_key(fan_id)).to_string()
    };
    let stored = name
        .as_deref()
        .and_then(fan::sanitize_name)
        .filter(|value| value != &default_name);
    state.session.set_fan_name(fan_id, stored);
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
            {
                let mut config = state.session.config.lock().unwrap();
                config.start_with_windows = !start_with_windows;
                let _ = config.save();
            }
            state.error = Some(error);
            state.error_at = Some(Instant::now());
        }
    }
}

fn persist_temp_source(state: &mut UiState, source: String) {
    let parsed = TemperatureSource::from_code(&source);
    let result = {
        let mut config = state.session.config.lock().unwrap();
        config.temperature_source = parsed.code().to_string();
        config.save()
    };
    if let Err(error) = result {
        state.error = Some(error);
        state.error_at = Some(Instant::now());
        return;
    }
    state.session.controller.set_preferred_temperature(parsed);
}

fn persist_smoothing(state: &mut UiState, window: u8) {
    state.session.set_smoothing_window(window);
}

fn persist_alert(state: &mut UiState, enabled: bool, celsius: u8) {
    let result = {
        let mut config = state.session.config.lock().unwrap();
        config.temp_alert_enabled = enabled;
        config.temp_alert_celsius = crate::alert::clamp_threshold(celsius);
        config.save()
    };
    if let Err(error) = result {
        state.error = Some(error);
        state.error_at = Some(Instant::now());
    }
}

fn export_config(state: &mut UiState, hwnd: isize) {
    let Some(path) = pick_json_file(hwnd, true) else {
        return;
    };
    let portable = state.session.config.lock().unwrap().to_portable();
    match serde_json::to_string_pretty(&portable) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                state.error = Some(error.to_string());
                state.error_at = Some(Instant::now());
            }
        }
        Err(error) => {
            state.error = Some(error.to_string());
            state.error_at = Some(Instant::now());
        }
    }
}

fn import_config(state: &mut UiState, shared: &Arc<Mutex<UiState>>, hwnd: isize) {
    let Some(path) = pick_json_file(hwnd, false) else {
        return;
    };
    let json = match std::fs::read_to_string(&path) {
        Ok(json) => json,
        Err(error) => {
            state.error = Some(error.to_string());
            state.error_at = Some(Instant::now());
            return;
        }
    };
    let portable = match PortableConfig::parse_json(&json) {
        Ok(portable) => portable,
        Err(error) => {
            state.error = Some(error);
            state.error_at = Some(Instant::now());
            return;
        }
    };
    let start_with_windows = portable.start_with_windows;
    let result = {
        let mut config = state.session.config.lock().unwrap();
        config.apply_portable(portable);
        config.save()
    };
    if let Err(error) = result {
        state.error = Some(error);
        state.error_at = Some(Instant::now());
        return;
    }
    let (preferred, smoothing_window) = {
        let config = state.session.config.lock().unwrap();
        (config.temperature_source(), config.smoothing_window)
    };
    state
        .session
        .controller
        .set_preferred_temperature(preferred);
    state
        .session
        .controller
        .set_smoothing_window(smoothing_window);
    if let Err(error) = set_start_with_windows(start_with_windows) {
        state.error = Some(error);
        state.error_at = Some(Instant::now());
        return;
    }
    state.session.restore_saved_state();
    refresh_metrics(shared);
}

fn maybe_temp_alert(app: &mut ControlApp) {
    let (enabled, threshold, temp, lang) = {
        let state = app.state.lock().unwrap();
        let config = state.session.config.lock().unwrap();
        (
            config.temp_alert_enabled,
            config.temp_alert_celsius,
            state.metrics.as_ref().map(|metrics| metrics.temperature),
            config.language(),
        )
    };
    let Some(temp) = temp else {
        return;
    };
    let now = Instant::now();
    if should_fire_temp_alert(enabled, threshold, temp, app.last_temp_alert, now) {
        app.last_temp_alert = Some(now);
        crate::tray::show_balloon(
            t(lang, "app_title"),
            &format!("{}{temp}\u{00B0}C", t(lang, "temp_alert_body")),
        );
    }
}

fn exit_now(state: &Arc<Mutex<UiState>>, stop: &Arc<AtomicBool>) {
    stop.store(true, Ordering::SeqCst);
    let session = Arc::clone(&state.lock().unwrap().session);
    crate::tray::shutdown();
    session.shutdown();
    std::process::exit(0);
}

fn refresh_metrics(state: &Arc<Mutex<UiState>>) {
    let session = Arc::clone(&state.lock().unwrap().session);
    let state = Arc::clone(state);
    thread::spawn(move || {
        if let Ok(metrics) = session.metrics() {
            let mut ui = state.lock().unwrap();
            ui.charts.push(&metrics);
            ui.metrics = Some(metrics);
        }
    });
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
            }
            Err(error) => {
                ui.error = Some(error);
                ui.error_at = Some(Instant::now());
            }
        }
        ui.edit.apu_applying = false;
    });
}

fn restore_fan_curve(state: &mut UiState, shared: &Arc<Mutex<UiState>>, index: usize, level: u8) {
    let defaults = FanConfig::default_for_fan((index + 1) as u8);
    state.edit.temp_fan_rampup[index] = defaults.rampup_curve;
    state.edit.temp_fan_rampdown[index] = defaults.rampdown_curve;
    state.edit.curve_dirty[index] = true;
    apply_fan(
        state,
        shared,
        index,
        "curve".into(),
        level,
        Some(defaults.rampup_curve),
        Some(defaults.rampdown_curve),
    );
}

fn apply_fan(
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
    index: usize,
    mode: String,
    level: u8,
    rampup: Option<[u8; 5]>,
    rampdown: Option<[u8; 5]>,
) {
    state.edit.fan_applying[index] = true;
    state.edit.dragging_level[index] = false;
    state.edit.dragging_curve[index] = false;
    let session = Arc::clone(&state.session);
    let shared = Arc::clone(shared);
    thread::spawn(move || {
        let result = session
            .apply_fan_edit((index + 1) as u8, &mode, level, rampup, rampdown)
            .and_then(|_| session.metrics());
        let mut ui = shared.lock().unwrap();
        match result {
            Ok(metrics) => {
                ui.metrics = Some(metrics);
                ui.edit.curve_dirty[index] = false;
            }
            Err(error) => {
                ui.error = Some(error);
                ui.error_at = Some(Instant::now());
            }
        }
        ui.edit.fan_applying[index] = false;
    });
}

fn settings_row(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(32.0);
        ui.spacing_mut().item_spacing.x = 12.0;
        add_contents(ui);
    });
}

fn settings_button_width(ui: &egui::Ui) -> f32 {
    ((ui.available_width() - 12.0) / 2.0).max(32.0)
}

fn segmented(
    ui: &mut egui::Ui,
    theme: &Theme,
    count: usize,
    add_contents: impl FnOnce(&mut egui::Ui, f32),
) {
    egui::Frame::NONE
        .fill(theme.control)
        .stroke(Stroke::new(1.0, theme.stroke))
        .corner_radius(CARD_RADIUS)
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal(|ui| {
                let gaps = (count.saturating_sub(1)) as f32 * 4.0;
                let width = ((ui.available_width() - gaps) / count.max(1) as f32).max(32.0);
                add_contents(ui, width);
            });
        });
}

fn hairline(ui: &mut egui::Ui, theme: &Theme) {
    ui.add_space(8.0);
    let rect = ui.available_rect_before_wrap();
    let y = ui.cursor().top();
    ui.painter()
        .hline(rect.x_range(), y, Stroke::new(1.0, theme.stroke));
    ui.add_space(9.0);
}

fn mode_button(ui: &mut egui::Ui, label: &str, selected: bool, width: f32) -> bool {
    ui.add_sized(
        [width, 28.0],
        egui::Button::selectable(selected, label).corner_radius(CONTROL_RADIUS),
    )
    .clicked()
}

fn nav_icon(ui: &mut egui::Ui, theme: &Theme, glyph: &str, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let pressed = response.is_pointer_button_down_on();
    if pressed {
        painter.rect_filled(
            egui::Rect::from_center_size(rect.center(), rect.size() * 0.96),
            CONTROL_RADIUS,
            theme.control_hover.lerp_to_gamma(Color32::BLACK, 0.08),
        );
    } else if response.hovered() {
        painter.rect_filled(rect, CONTROL_RADIUS, theme.control_hover);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            CONTROL_RADIUS,
            Stroke::new(2.0, theme.accent),
            egui::StrokeKind::Outside,
        );
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(16.0, Theme::icons()),
        theme.text,
    );
    response.on_hover_text(tip).clicked()
}

fn toggle_switch(ui: &mut egui::Ui, theme: &Theme, on: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(40.0, 32.0), egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let amount = ui.ctx().animate_bool_with_time_and_easing(
        response.id,
        *on,
        0.15,
        egui::emath::easing::cubic_out,
    );
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(40.0, 20.0));
    let fill = theme.control_hover.lerp_to_gamma(theme.accent, amount);
    let painter = ui.painter_at(rect);
    painter.rect_filled(track, 10.0, fill);
    if response.has_focus() {
        painter.rect_stroke(
            track,
            10.0,
            Stroke::new(2.0, theme.accent),
            egui::StrokeKind::Outside,
        );
    }
    let thumb_radius = if response.is_pointer_button_down_on() {
        6.0 * 0.96
    } else {
        6.0
    };
    let x = egui::lerp((track.left() + 10.0)..=(track.right() - 10.0), amount);
    painter.circle_filled(
        egui::pos2(x, track.center().y),
        thumb_radius,
        Color32::WHITE,
    );
    response
}

#[allow(clippy::too_many_arguments)]
fn draw_curve_table(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    index: usize,
    current_level: u8,
    applying: bool,
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
) {
    let mut any_dragged = false;
    let mut should_commit = false;
    ui.add_enabled_ui(!applying, |ui| {
        egui::Grid::new(format!("curve_table_{index}"))
            .num_columns(3)
            .spacing([8.0, 4.0])
            .min_col_width(52.0)
            .show(ui, |ui| {
                ui.label(RichText::new(t(lang, "level")).small().color(theme.muted));
                ui.label(RichText::new(t(lang, "ramp_up")).small().color(theme.muted));
                ui.label(
                    RichText::new(t(lang, "ramp_down"))
                        .small()
                        .color(theme.muted),
                );
                ui.end_row();

                for point in 0..5 {
                    ui.label(
                        RichText::new(format!("{point}\u{2192}{}", point + 1)).color(theme.muted),
                    );
                    let mut value = i32::from(state.edit.temp_fan_rampup[index][point]);
                    let min = if point == 0 {
                        0
                    } else {
                        i32::from(state.edit.temp_fan_rampup[index][point - 1]) + 1
                    };
                    let max = if point + 1 >= 5 {
                        i32::from(CURVE_TEMP_MAX)
                    } else {
                        i32::from(state.edit.temp_fan_rampup[index][point + 1]).saturating_sub(1)
                    };
                    let (lo, hi) = if min > max {
                        (value, value)
                    } else {
                        (min, max)
                    };
                    let response = ui.add(
                        egui::DragValue::new(&mut value)
                            .range(lo..=hi)
                            .suffix("\u{00B0}C")
                            .speed(1.0),
                    );
                    if response.changed() {
                        let next = clamp_rampup_point(
                            state.edit.temp_fan_rampup[index],
                            point,
                            u8::try_from(value.clamp(0, i32::from(CURVE_TEMP_MAX)))
                                .unwrap_or(CURVE_TEMP_MAX),
                        );
                        if next != state.edit.temp_fan_rampup[index] {
                            state.edit.temp_fan_rampup[index] = next;
                            state.edit.temp_fan_rampdown[index] = derive_rampdown(&next);
                            state.edit.curve_dirty[index] = true;
                        }
                    }
                    if response.dragged() {
                        any_dragged = true;
                    }
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        should_commit = true;
                    }
                    let down = state.edit.temp_fan_rampdown[index][point];
                    ui.label(RichText::new(format!("\u{2193} {down}\u{00B0}C")).color(theme.muted));
                    ui.end_row();
                }
            });
    });
    state.edit.dragging_curve[index] = any_dragged;
    if should_commit && !any_dragged && state.edit.curve_dirty[index] {
        let rampup = state.edit.temp_fan_rampup[index];
        let rampdown = derive_rampdown(&rampup);
        state.edit.temp_fan_rampdown[index] = rampdown;
        apply_fan(
            state,
            shared,
            index,
            "curve".into(),
            current_level,
            Some(rampup),
            Some(rampdown),
        );
    }
}

fn draw_chart(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    history: &VecDeque<i32>,
    max_value: i32,
    color: Color32,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CONTROL_RADIUS, theme.control);
    painter.rect_stroke(
        rect,
        CONTROL_RADIUS,
        Stroke::new(1.0, theme.stroke),
        egui::StrokeKind::Inside,
    );
    if history.len() < 2 {
        return;
    }
    let inner = rect.shrink(3.0);
    let max_value = max_value.max(1) as f32;
    let line: Vec<_> = history
        .iter()
        .enumerate()
        .map(|(i, &value)| chart_point(inner, i, history.len(), value, max_value))
        .collect();
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 56);
    let bottom = inner.max.y;
    for pair in line.windows(2) {
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(pair[0].x, bottom),
                egui::pos2(pair[1].x, bottom),
                pair[1],
                pair[0],
            ],
            fill,
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(
        line,
        Stroke::new(1.5, color.gamma_multiply(0.85)),
    ));
}

fn chart_point(
    inner: egui::Rect,
    index: usize,
    len: usize,
    value: i32,
    max_value: f32,
) -> egui::Pos2 {
    let x = inner.min.x + inner.width() * index as f32 / (len - 1) as f32;
    let y = inner.max.y - inner.height() * (value as f32 / max_value).clamp(0.0, 1.0);
    egui::pos2(x, y)
}

fn sensor_chip(ui: &mut egui::Ui, theme: &Theme, label: &str, value: Option<u8>) {
    let Some(temp) = value else {
        return;
    };
    ui.label(
        RichText::new(format!("{label} {temp}\u{00B0}C"))
            .small()
            .color(temp_color(theme, temp))
            .family(FontFamily::Monospace),
    );
}

fn temp_color(theme: &Theme, temp: u8) -> Color32 {
    if temp <= 50 {
        theme.ok
    } else if temp <= 85 {
        theme.warn
    } else {
        theme.bad
    }
}

fn rpm_color(theme: &Theme, rpm: u16) -> Color32 {
    if rpm <= 1250 {
        theme.ok
    } else if rpm <= 2500 {
        theme.warn
    } else {
        theme.bad
    }
}

fn configure_window(app: &mut ControlApp, ctx: &egui::Context) {
    if app.window_configured {
        return;
    }
    let size = egui::Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT);
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
    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
        WINDOW_WIDTH,
        WINDOW_MIN_HEIGHT,
    )));
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
        (screen[0] - size.x) / 2.0,
        (screen[1] - size.y) / 2.0,
    )));
    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
    app.window_configured = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hides_to_tray_unless_quitting() {
        assert!(hide_on_close(true, false));
        assert!(!hide_on_close(true, true));
        assert!(!hide_on_close(false, false));
        assert!(!hide_on_close(false, true));
    }

    #[test]
    fn hidden_to_tray_skips_ui_paint_and_slows_repaint() {
        let policy = idle_paint_policy(true);
        assert!(!policy.paint_ui);
        assert!(policy.repaint_after >= Duration::from_secs(1));
    }

    #[test]
    fn visible_window_keeps_live_refresh() {
        let policy = idle_paint_policy(false);
        assert!(policy.paint_ui);
        assert_eq!(policy.repaint_after, Duration::from_millis(200));
    }

    #[test]
    fn window_is_enlarged_forty_percent_from_compact_size() {
        assert!((WINDOW_WIDTH - 480.0 * 0.6 * 1.4).abs() < f32::EPSILON);
        assert!((WINDOW_HEIGHT - 640.0 * 0.6 * 1.4).abs() < f32::EPSILON);
    }

    fn light_theme() -> Theme {
        Theme::from_chrome(UiChrome {
            dark: false,
            accent: (0, 120, 212),
        })
    }

    #[test]
    fn semantic_metric_colors_follow_thresholds() {
        let theme = light_theme();
        assert_eq!(temp_color(&theme, 50), theme.ok);
        assert_eq!(temp_color(&theme, 51), theme.warn);
        assert_eq!(temp_color(&theme, 86), theme.bad);
        assert_eq!(rpm_color(&theme, 1250), theme.ok);
        assert_eq!(rpm_color(&theme, 1251), theme.warn);
        assert_eq!(rpm_color(&theme, 2501), theme.bad);
    }

    #[test]
    fn chart_point_maps_history_extents() {
        let inner = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 50.0));
        assert_eq!(chart_point(inner, 0, 2, 0, 100.0), egui::pos2(0.0, 50.0));
        assert_eq!(chart_point(inner, 1, 2, 100, 100.0), egui::pos2(100.0, 0.0));
    }
}
