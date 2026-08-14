use std::mem;
use std::ptr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use winapi::shared::minwindef::{HINSTANCE, LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HICON, HMENU, HWND, POINT};
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::shellapi::{
    ExtractIconW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use winapi::um::winuser::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, PostMessageW, PostQuitMessage,
    RegisterClassW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, CW_USEDEFAULT,
    MF_STRING, MSG, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
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
    SetPowerMode(String),
    Exit,
}

struct TrayInner {
    events: Sender<TrayEvent>,
    session: Arc<AppSession>,
}

static TRAY: OnceLock<Mutex<Option<TrayInner>>> = OnceLock::new();

pub fn start(session: Arc<AppSession>) -> Receiver<TrayEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || message_loop(session, tx));
    for _ in 0..50 {
        if find_hwnd().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    rx
}

pub fn request_show() -> bool {
    for _ in 0..30 {
        if let Some(hwnd) = find_hwnd() {
            return unsafe { PostMessageW(hwnd, SHOW_MSG, 0, 0) != 0 };
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

pub fn shutdown() {
    if let Some(hwnd) = find_hwnd() {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
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

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("EVO-X2 Control").as_ptr(),
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
        let mut tip = [0u16; 128];
        copy_wide(&mut tip, "EVO-X2 Control");
        let mut nid: NOTIFYICONDATAW = mem::zeroed();
        nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as UINT;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = CALLBACK_MSG;
        nid.hIcon = icon;
        nid.szTip = tip;
        Shell_NotifyIconW(NIM_ADD, &mut nid);

        *TRAY.get_or_init(|| Mutex::new(None)).lock().unwrap() =
            Some(TrayInner { events, session });

        let mut msg = mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Shell_NotifyIconW(NIM_DELETE, &mut nid);
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
    match msg {
        CALLBACK_MSG => {
            let mouse = lparam as UINT;
            if mouse == WM_LBUTTONUP {
                send_event(TrayEvent::ShowWindow);
            } else if mouse == WM_RBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        SHOW_MSG => {
            send_event(TrayEvent::ShowWindow);
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                ID_QUIET => send_event(TrayEvent::SetPowerMode("quiet".into())),
                ID_BALANCED => send_event(TrayEvent::SetPowerMode("balanced".into())),
                ID_PERFORMANCE => send_event(TrayEvent::SetPowerMode("performance".into())),
                ID_SHOW => send_event(TrayEvent::ShowWindow),
                ID_EXIT => {
                    send_event(TrayEvent::Exit);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
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
        append(menu, ID_SHOW, t(language, "show_window"));
        append(menu, ID_EXIT, t(language, "exit"));
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            ptr::null(),
        );
        DestroyMenu(menu);
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
        .unwrap_or(Language::Zh)
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

fn copy_wide(dest: &mut [u16], text: &str) {
    let encoded: Vec<u16> = text.encode_utf16().collect();
    let len = encoded.len().min(dest.len().saturating_sub(1));
    dest[..len].copy_from_slice(&encoded[..len]);
    dest[len] = 0;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
