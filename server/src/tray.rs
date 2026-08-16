use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use eframe::egui;
use winapi::shared::minwindef::{HINSTANCE, LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HICON, HMENU, HWND, POINT};
use winapi::um::consoleapi::SetConsoleCtrlHandler;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::processthreadsapi::{GetCurrentProcessId, GetCurrentThreadId};
use winapi::um::shellapi::{
    ExtractIconW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP,
    NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_KEYSELECT, NIN_SELECT,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use winapi::um::wincon::{CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT};
use winapi::um::winuser::{
    AppendMenuW, AttachThreadInput, BringWindowToTop, CreatePopupMenu, CreateWindowExW,
    DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, EnumWindows, FindWindowW,
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, PostMessageW, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, ShowWindow, TrackPopupMenu, TranslateMessage,
    CW_USEDEFAULT, MF_SEPARATOR, MF_STRING, MSG, SW_RESTORE, SW_SHOW, TPM_BOTTOMALIGN,
    TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::i18n::{t, Language};
use crate::platform::current_exe_path;
use crate::session::AppSession;

const CLASS_NAME: &str = "EVOX2ControlMsgWnd";
const CALLBACK_MSG: UINT = WM_APP + 1;
const SHOW_MSG: UINT = WM_APP + 2;
const TRAY_ID: UINT = 1;
const ID_QUIET: usize = 1001;
const ID_BALANCED: usize = 1002;
const ID_PERFORMANCE: usize = 1003;
const ID_SHOW: usize = 1004;
const ID_EXIT: usize = 1005;

#[derive(Debug, Clone)]
pub enum TrayEvent {
    ShowWindow,
    PowerModeChanged,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayPointerAction {
    Show,
    Menu,
    Ignore,
}

struct TrayInner {
    events: Sender<TrayEvent>,
    session: Arc<AppSession>,
}

struct NotifyState {
    hwnd: isize,
    icon: isize,
}

unsafe impl Send for NotifyState {}

static TRAY: OnceLock<Mutex<Option<TrayInner>>> = OnceLock::new();
static NOTIFY: OnceLock<Mutex<Option<NotifyState>>> = OnceLock::new();
static GUI_CTX: Mutex<Option<egui::Context>> = Mutex::new(None);
static GUI_HWND: AtomicIsize = AtomicIsize::new(0);
static FORCE_QUIT: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn start(session: Arc<AppSession>) -> Receiver<TrayEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || message_loop(session, tx));
    for _ in 0..50 {
        if find_hwnd().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    register_shutdown_handler();
    rx
}

fn register_shutdown_handler() {
    unsafe {
        SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);
    }
}

unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> i32 {
    match ctrl_type {
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            if let Some(session) = current_session() {
                session.shutdown();
            }
            1
        }
        _ => 0,
    }
}

pub fn bind_gui(ctx: egui::Context, hwnd: isize) {
    *GUI_CTX.lock().unwrap() = Some(ctx);
    if hwnd != 0 {
        GUI_HWND.store(hwnd, Ordering::SeqCst);
    }
}

pub fn set_gui_hwnd(hwnd: isize) {
    if hwnd != 0 {
        GUI_HWND.store(hwnd, Ordering::SeqCst);
    }
}

pub fn is_quitting() -> bool {
    FORCE_QUIT.load(Ordering::SeqCst)
}

pub fn request_show() -> bool {
    for _ in 0..30 {
        if let Some(hwnd) = find_hwnd() {
            let posted = unsafe { PostMessageW(hwnd, SHOW_MSG, 0, 0) != 0 };
            show_main_window();
            return posted;
        }
        thread::sleep(Duration::from_millis(50));
    }
    show_main_window();
    false
}

pub fn shutdown() {
    delete_icon();
    if let Some(hwnd) = find_hwnd() {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}

pub fn show_balloon(title: &str, text: &str) {
    let Some(state) = NOTIFY
        .get()
        .and_then(|notify| notify.lock().ok())
        .and_then(|guard| {
            guard.as_ref().map(|state| NotifyState {
                hwnd: state.hwnd,
                icon: state.icon,
            })
        })
    else {
        return;
    };
    unsafe {
        let mut nid = notify_data(state.hwnd as HWND, state.icon as HICON);
        nid.uFlags |= NIF_INFO;
        copy_wide(&mut nid.szInfoTitle, title);
        copy_wide(&mut nid.szInfo, text);
        nid.dwInfoFlags = NIIF_INFO;
        Shell_NotifyIconW(NIM_MODIFY, &mut nid);
    }
}

fn message_loop(session: Arc<AppSession>, events: Sender<TrayEvent>) {
    unsafe {
        let instance = GetModuleHandleW(ptr::null()) as HINSTANCE;
        let class = wide(CLASS_NAME);
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);

        let taskbar = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        TASKBAR_CREATED.store(taskbar, Ordering::SeqCst);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide(crate::i18n::APP_NAME).as_ptr(),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        );
        if hwnd.is_null() {
            return;
        }

        let icon = load_icon(instance);
        *NOTIFY.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(NotifyState {
            hwnd: hwnd as isize,
            icon: icon as isize,
        });
        add_icon();

        *TRAY.get_or_init(|| Mutex::new(None)).lock().unwrap() =
            Some(TrayInner { events, session });

        let mut msg = mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        delete_icon();
        if let Some(tray) = TRAY.get() {
            *tray.lock().unwrap() = None;
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let taskbar = TASKBAR_CREATED.load(Ordering::SeqCst);
    if taskbar != 0 && msg == taskbar {
        add_icon();
        return 0;
    }
    match msg {
        CALLBACK_MSG => {
            match tray_pointer_action(true, lparam as u32) {
                TrayPointerAction::Show => show_main_window(),
                TrayPointerAction::Menu => show_menu(hwnd),
                TrayPointerAction::Ignore => {}
            }
            0
        }
        SHOW_MSG => {
            show_main_window();
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                ID_QUIET => apply_power_mode("quiet"),
                ID_BALANCED => apply_power_mode("balanced"),
                ID_PERFORMANCE => apply_power_mode("performance"),
                ID_SHOW => show_main_window(),
                ID_EXIT => quit_now(),
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            delete_icon();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub(crate) fn tray_pointer_action(notify_version4: bool, lparam: u32) -> TrayPointerAction {
    let message = if notify_version4 {
        lparam & 0xFFFF
    } else {
        lparam
    };
    match message {
        NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP | WM_LBUTTONDBLCLK => TrayPointerAction::Show,
        WM_LBUTTONDOWN if !notify_version4 => TrayPointerAction::Show,
        WM_CONTEXTMENU | WM_RBUTTONUP => TrayPointerAction::Menu,
        _ => TrayPointerAction::Ignore,
    }
}

fn show_main_window() {
    send_event(TrayEvent::ShowWindow);
    if let Some(ctx) = GUI_CTX.lock().unwrap().as_ref() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
    show_native_window();
}

fn show_native_window() {
    let hwnd = resolve_gui_hwnd();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        } else {
            ShowWindow(hwnd, SW_SHOW);
        }
        let foreground = GetForegroundWindow();
        let this_thread = GetCurrentThreadId();
        let mut foreground_pid = 0;
        let foreground_thread = GetWindowThreadProcessId(foreground, &mut foreground_pid);
        if foreground_thread != 0 && foreground_thread != this_thread {
            AttachThreadInput(this_thread, foreground_thread, 1);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            AttachThreadInput(this_thread, foreground_thread, 0);
        } else {
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
        }
    }
}

fn apply_power_mode(mode: &'static str) {
    if let Some(session) = current_session() {
        thread::spawn(move || {
            let _ = session.set_power_mode(mode);
            send_event(TrayEvent::PowerModeChanged);
            wake_gui();
        });
    }
}

fn quit_now() {
    FORCE_QUIT.store(true, Ordering::SeqCst);
    send_event(TrayEvent::Exit);
    wake_gui();
    delete_icon();
    if let Some(session) = current_session() {
        session.shutdown();
    }
    std::process::exit(0);
}

fn wake_gui() {
    if let Some(ctx) = GUI_CTX.lock().unwrap().as_ref() {
        ctx.request_repaint();
    }
}

fn current_session() -> Option<Arc<AppSession>> {
    TRAY.get()
        .and_then(|tray| tray.lock().ok())
        .and_then(|guard| guard.as_ref().map(|inner| Arc::clone(&inner.session)))
}

fn resolve_gui_hwnd() -> HWND {
    let stored = GUI_HWND.load(Ordering::SeqCst) as HWND;
    if !stored.is_null() && unsafe { IsWindow(stored) } != 0 {
        return stored;
    }
    let mut found: HWND = ptr::null_mut();
    unsafe {
        EnumWindows(Some(enum_gui_windows), &mut found as *mut HWND as LPARAM);
    }
    if !found.is_null() {
        GUI_HWND.store(found as isize, Ordering::SeqCst);
    }
    found
}

unsafe extern "system" fn enum_gui_windows(hwnd: HWND, lparam: LPARAM) -> i32 {
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != GetCurrentProcessId() {
        return 1;
    }
    let mut class = [0u16; 256];
    let class_len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
    if class_len <= 0 {
        return 1;
    }
    let class_name = String::from_utf16_lossy(&class[..class_len as usize]);
    if class_name == CLASS_NAME {
        return 1;
    }
    let mut title = [0u16; 256];
    let title_len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
    if title_len <= 0 {
        return 1;
    }
    *(lparam as *mut HWND) = hwnd;
    0
}

fn show_menu(hwnd: HWND) {
    let language = current_language();
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        append(menu, ID_QUIET, t(language, "quiet"));
        append(menu, ID_BALANCED, t(language, "balanced"));
        append(menu, ID_PERFORMANCE, t(language, "performance"));
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        append(menu, ID_SHOW, t(language, "show_window"));
        append(menu, ID_EXIT, t(language, "exit"));
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            0,
            hwnd,
            ptr::null(),
        );
        DestroyMenu(menu);
        PostMessageW(hwnd, 0, 0, 0);
    }
}

fn append(menu: HMENU, id: usize, text: &str) {
    let wide = wide(text);
    unsafe {
        AppendMenuW(menu, MF_STRING, id, wide.as_ptr());
    }
}

fn send_event(event: TrayEvent) {
    if let Some(tray) = TRAY.get() {
        if let Ok(guard) = tray.lock() {
            if let Some(inner) = guard.as_ref() {
                let _ = inner.events.send(event);
            }
        }
    }
}

fn current_language() -> Language {
    TRAY.get()
        .and_then(|tray| tray.lock().ok())
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|inner| inner.session.config.lock().unwrap().language())
        })
        .unwrap_or(Language::En)
}

fn add_icon() {
    let Some(state) = NOTIFY
        .get()
        .and_then(|notify| notify.lock().ok())
        .and_then(|guard| {
            guard.as_ref().map(|state| NotifyState {
                hwnd: state.hwnd,
                icon: state.icon,
            })
        })
    else {
        return;
    };
    unsafe {
        let mut nid = notify_data(state.hwnd as HWND, state.icon as HICON);
        for _ in 0..40 {
            if Shell_NotifyIconW(NIM_ADD, &mut nid) != 0 {
                *nid.u.uVersion_mut() = NOTIFYICON_VERSION_4;
                Shell_NotifyIconW(NIM_SETVERSION, &mut nid);
                return;
            }
            thread::sleep(Duration::from_millis(250));
        }
    }
}

fn delete_icon() {
    let Some(state) = NOTIFY.get().and_then(|notify| notify.lock().ok()) else {
        return;
    };
    let Some(state) = state.as_ref() else {
        return;
    };
    unsafe {
        let mut nid = notify_data(state.hwnd as HWND, state.icon as HICON);
        Shell_NotifyIconW(NIM_DELETE, &mut nid);
    }
}

fn notify_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    unsafe {
        let mut tip = [0u16; 128];
        copy_wide(&mut tip, crate::i18n::APP_NAME);
        let mut nid: NOTIFYICONDATAW = mem::zeroed();
        nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as UINT;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        nid.uCallbackMessage = CALLBACK_MSG;
        nid.hIcon = icon;
        nid.szTip = tip;
        nid
    }
}

fn load_icon(instance: HINSTANCE) -> HICON {
    if let Ok(path) = current_exe_path() {
        let wide = wide(&path);
        let icon = unsafe { ExtractIconW(instance, wide.as_ptr(), 0) };
        if !icon.is_null() && icon != (-1isize as HICON) {
            return icon;
        }
    }
    ptr::null_mut()
}

fn find_hwnd() -> Option<HWND> {
    let class = wide(CLASS_NAME);
    let hwnd = unsafe { FindWindowW(class.as_ptr(), ptr::null()) };
    if hwnd.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

fn copy_wide(dest: &mut [u16], text: &str) {
    let encoded: Vec<u16> = text.encode_utf16().collect();
    let len = encoded.len().min(dest.len().saturating_sub(1));
    dest[..len].copy_from_slice(&encoded[..len]);
    dest[len] = 0;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version4_left_click_shows_window() {
        assert_eq!(
            tray_pointer_action(true, NIN_SELECT),
            TrayPointerAction::Show
        );
        assert_eq!(
            tray_pointer_action(true, NIN_KEYSELECT),
            TrayPointerAction::Show
        );
        assert_eq!(
            tray_pointer_action(true, WM_LBUTTONDBLCLK),
            TrayPointerAction::Show
        );
    }

    #[test]
    fn version4_context_menu_opens_menu() {
        assert_eq!(
            tray_pointer_action(true, WM_CONTEXTMENU),
            TrayPointerAction::Menu
        );
        assert_eq!(
            tray_pointer_action(true, WM_RBUTTONUP),
            TrayPointerAction::Menu
        );
    }

    #[test]
    fn legacy_left_button_still_shows_window() {
        assert_eq!(
            tray_pointer_action(false, WM_LBUTTONUP),
            TrayPointerAction::Show
        );
        assert_eq!(
            tray_pointer_action(false, WM_LBUTTONDOWN),
            TrayPointerAction::Show
        );
    }
}
