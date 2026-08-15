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

use crate::i18n::{t, Language};
use crate::platform::{set_start_with_windows, ui_chrome, UiChrome};
use crate::session::{AppSession, FanSnapshot, MetricsSnapshot};
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
    temp_fan_rampup: [String; 3],
    temp_fan_rampdown: [String; 3],
    curve_dirty: [bool; 3],
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            apu_applying: false,
            fan_applying: [false; 3],
            temp_fan_level: [0, 0, 0],
            dragging_level: [false; 3],
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
            curve_dirty: [false; 3],
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
}

struct ControlApp {
    state: Arc<Mutex<UiState>>,
    stop: Arc<AtomicBool>,
    window_configured: bool,
    last_chrome: Option<UiChrome>,
    tray_rx: Receiver<TrayEvent>,
    page: Page,
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
        page: Page::Home,
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

    let mut style = (*ctx.style()).clone();
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
    ctx.set_style(style);
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
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let fonts = std::path::Path::new(&windir).join("Fonts");
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
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let fonts = std::path::Path::new(&windir).join("Fonts");
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

impl eframe::App for ControlApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        crate::tray::set_gui_hwnd(hwnd_from_handle(&frame.window_handle()));
        let chrome = ui_chrome();
        if self.last_chrome != Some(chrome) {
            let theme = Theme::from_chrome(chrome);
            install_theme(ctx, &theme);
            self.last_chrome = Some(chrome);
        }
        let theme = Theme::from_chrome(chrome);

        while let Ok(event) = self.tray_rx.try_recv() {
            match event {
                TrayEvent::ShowWindow => {
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
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                exit_now(&self.state, &self.stop);
            }
        }

        if self.page == Page::Settings && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.page = Page::Home;
        }

        let shared = Arc::clone(&self.state);
        let mut next_page = self.page;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme.bg)
                    .inner_margin(Margin::same(10)),
            )
            .show(ctx, |ui| {
                let mut state = self.state.lock().unwrap();
                if let (Some(_), Some(timestamp)) = (&state.error, state.error_at) {
                    if timestamp.elapsed() > Duration::from_secs(5) {
                        state.error = None;
                        state.error_at = None;
                    }
                }

                let lang = state.session.config.lock().unwrap().language();
                let page = next_page;
                let scroll_id = match page {
                    Page::Home => "home-scroll",
                    Page::Settings => "settings-scroll",
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt(scroll_id)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match page {
                            Page::Home => {
                                draw_header(ui, lang, &theme, &state, &mut next_page);
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
                                draw_settings_header(ui, lang, &theme, &mut next_page);
                                draw_error(ui, lang, &theme, &state);
                                ui.add_space(10.0);
                                draw_settings(ui, lang, &theme, ctx, &mut state);
                            }
                        }
                    });
            });
        self.page = next_page;

        configure_window(self, ctx);
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop.store(true, Ordering::SeqCst);
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

fn draw_header(ui: &mut egui::Ui, lang: Language, theme: &Theme, state: &UiState, page: &mut Page) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t(lang, "app_title"))
                .size(16.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if nav_icon(ui, theme, ICON_SETTING, t(lang, "settings")) {
                *page = Page::Settings;
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

fn draw_settings_header(ui: &mut egui::Ui, lang: Language, theme: &Theme, page: &mut Page) {
    ui.horizontal(|ui| {
        if nav_icon(ui, theme, ICON_BACK, t(lang, "back")) {
            *page = Page::Home;
        }
        ui.label(
            RichText::new(t(lang, "settings"))
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
    card(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("APU");
            if state.edit.apu_applying {
                ui.add(egui::Spinner::new().size(14.0));
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
    let name = t(
        lang,
        match index {
            0 => "fan1",
            1 => "fan2",
            _ => "fan3",
        },
    );
    let fan = &metrics.fans[index];
    let history = state.charts.fans[index].clone();
    let max_rpm = if index == 2 { 2500 } else { 5000 };
    if !state.edit.dragging_level[index] {
        state.edit.temp_fan_level[index] = i32::from(fan.level);
    }
    if !state.edit.curve_dirty[index] {
        state.edit.temp_fan_rampup[index] = join_curve(&fan.rampup_curve);
        state.edit.temp_fan_rampdown[index] = join_curve(&fan.rampdown_curve);
    }
    let applying = state.edit.fan_applying[index];

    card(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(name);
            ui.label(RichText::new(t(lang, &fan.mode)).small().color(theme.muted));
            if applying {
                ui.add(egui::Spinner::new().size(14.0));
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
            ui.horizontal(|ui| {
                ui.label(RichText::new(t(lang, "ramp_up")).color(theme.muted));
                ui.add_enabled_ui(!applying, |ui| {
                    let response = ui.text_edit_singleline(&mut state.edit.temp_fan_rampup[index]);
                    if response.changed() {
                        state.edit.curve_dirty[index] = true;
                    }
                    if response.lost_focus() && state.edit.curve_dirty[index] {
                        commit_fan_curve(state, shared, index, fan);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t(lang, "ramp_down")).color(theme.muted));
                ui.add_enabled_ui(!applying, |ui| {
                    let response =
                        ui.text_edit_singleline(&mut state.edit.temp_fan_rampdown[index]);
                    if response.changed() {
                        state.edit.curve_dirty[index] = true;
                    }
                    if response.lost_focus() && state.edit.curve_dirty[index] {
                        commit_fan_curve(state, shared, index, fan);
                    }
                });
            });
            ui.label(
                RichText::new(t(lang, "hint_curve"))
                    .small()
                    .color(theme.muted),
            );
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

fn draw_settings(
    ui: &mut egui::Ui,
    lang: Language,
    theme: &Theme,
    ctx: &egui::Context,
    state: &mut UiState,
) {
    let mut close_to_tray;
    let mut start_with_windows;
    let mut language;
    {
        let config = state.session.config.lock().unwrap();
        close_to_tray = config.close_to_tray;
        start_with_windows = config.start_with_windows;
        language = config.language.clone();
    }

    card(theme).show(ui, |ui| {
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

fn commit_fan_curve(
    state: &mut UiState,
    shared: &Arc<Mutex<UiState>>,
    index: usize,
    fan: &FanSnapshot,
) {
    let lang = state.session.config.lock().unwrap().language();
    let rampup = parse_curve(&state.edit.temp_fan_rampup[index]);
    let rampdown = parse_curve(&state.edit.temp_fan_rampdown[index]);
    match (rampup, rampdown) {
        (Some(rampup), Some(rampdown)) => {
            apply_fan(
                state,
                shared,
                index,
                "curve".into(),
                fan.level,
                Some(rampup),
                Some(rampdown),
            );
        }
        (None, _) => {
            state.error = Some(t(lang, "error_rampup").to_string());
            state.error_at = Some(Instant::now());
        }
        (_, None) => {
            state.error = Some(t(lang, "error_rampdown").to_string());
            state.error_at = Some(Instant::now());
        }
    }
}

fn settings_row(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        ui.spacing_mut().item_spacing.x = 12.0;
        add_contents(ui);
    });
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
