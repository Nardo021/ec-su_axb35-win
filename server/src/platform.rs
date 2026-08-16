use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr;
use std::sync::OnceLock;

use std::mem;

use winapi::shared::minwindef::{DWORD, HKEY};
use winapi::shared::windef::HWND;
use winapi::shared::winerror::{ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use winapi::um::commdlg::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::CloseHandle;
use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::shellapi::ShellExecuteW;
use winapi::um::stringapiset::MultiByteToWideChar;
use winapi::um::synchapi::{
    CreateEventW, CreateMutexW, OpenEventW, OpenMutexW, ReleaseMutex, SetEvent, WaitForSingleObject,
};
use winapi::um::sysinfoapi::{ComputerNameDnsHostname, GetComputerNameExW};
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::um::winnls::GetACP;
use winapi::um::winnt::{
    HANDLE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_DWORD, REG_SZ, SYNCHRONIZE,
    TOKEN_ELEVATION, TOKEN_QUERY,
};
use winapi::um::winreg::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE,
};
use winapi::um::winuser::SW_SHOWNORMAL;

use crate::hardware::read_reg_string;

const WAIT_ABANDONED: DWORD = 0x00000080;
const ACCESS_EC_TIMEOUT_MS: DWORD = 5_000;
pub const ALREADY_RUNNING: &str = "ALREADY_RUNNING";
const INSTANCE_MUTEX_NAME: &str = "Global\\EVOX2-Control";
const RELOAD_FANS_EVENT_NAME: &str = "Global\\EVOX2-Control-ReloadFans";
const EVENT_MODIFY_STATE: DWORD = 0x0002;
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "EVO-X2 Control";
const STARTUP_TASK_NAME: &str = "EVO-X2 Control";
const STARTUP_APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn host_name() -> String {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(detect_host_name).clone()
}

fn detect_host_name() -> String {
    dns_host_name()
        .or_else(env_computer_name)
        .unwrap_or_else(|| "Processor".to_string())
}

fn env_computer_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn dns_host_name() -> Option<String> {
    unsafe {
        let mut size = 0u32;
        GetComputerNameExW(ComputerNameDnsHostname, ptr::null_mut(), &mut size);
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0u16; size as usize];
        if GetComputerNameExW(ComputerNameDnsHostname, buffer.as_mut_ptr(), &mut size) == 0 {
            return None;
        }
        let name = String::from_utf16_lossy(&buffer[..size as usize]);
        let name = name.trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    }
}

pub fn is_admin() -> bool {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
        let result = GetTokenInformation(
            token,
            winapi::um::winnt::TokenElevation,
            &mut elevation as *mut _ as *mut _,
            size,
            &mut size,
        );
        CloseHandle(token);
        result != 0 && elevation.TokenIsElevated != 0
    }
}

/// Informational only. Never changes Secure Boot configuration.
pub fn secure_boot_status() -> String {
    match read_reg_dword(
        r"SYSTEM\CurrentControlSet\Control\SecureBoot\State",
        "UEFISecureBootEnabled",
    ) {
        Some(1) => "Enabled".to_string(),
        Some(0) => "Disabled".to_string(),
        Some(value) => format!("Unknown ({value})"),
        None => "Unknown".to_string(),
    }
}

pub fn pawnio_install_version() -> Option<String> {
    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO";
    const UNINSTALL_WOW: &str =
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO";
    read_reg_string(HKEY_LOCAL_MACHINE, UNINSTALL, "DisplayVersion")
        .or_else(|| read_reg_string(HKEY_LOCAL_MACHINE, UNINSTALL_WOW, "DisplayVersion"))
}

fn read_reg_dword(subkey: &str, value_name: &str) -> Option<u32> {
    use winapi::shared::winerror::ERROR_SUCCESS;
    use winapi::um::winnt::{KEY_READ, REG_DWORD};
    use winapi::um::winreg::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW};

    unsafe {
        let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut key = ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey_w.as_ptr(), 0, KEY_READ, &mut key)
            != ERROR_SUCCESS as i32
        {
            return None;
        }

        let name_w: Vec<u16> = value_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut kind: DWORD = 0;
        let mut data: DWORD = 0;
        let mut size = std::mem::size_of::<DWORD>() as DWORD;
        let status = RegQueryValueExW(
            key,
            name_w.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            &mut data as *mut DWORD as *mut u8,
            &mut size,
        );
        RegCloseKey(key);
        if status == ERROR_SUCCESS as i32 && kind == REG_DWORD {
            Some(data)
        } else {
            None
        }
    }
}

/// Serializes ACPI EC access with other PawnIO/LHM clients.
///
/// `LpcACPIEC.p` documents `\BaseNamedObjects\Access_EC`, which is the Win32
/// name `Global\Access_EC`.
pub struct AccessEcGuard {
    handle: HANDLE,
}

impl AccessEcGuard {
    pub fn acquire() -> Result<Self, String> {
        let name: Vec<u16> = "Global\\Access_EC"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(ptr::null_mut(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("Failed to create Access_EC mutex".to_string());
        }
        let wait = unsafe { WaitForSingleObject(handle, ACCESS_EC_TIMEOUT_MS) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            unsafe { CloseHandle(handle) };
            return Err("Timeout waiting for Access_EC mutex".to_string());
        }
        Ok(Self { handle })
    }
}

impl Drop for AccessEcGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

unsafe impl Send for AccessEcGuard {}

/// One long-lived owner of the EC stack (GUI or service). CLI does not take this.
pub struct InstanceGuard {
    handle: HANDLE,
}

impl InstanceGuard {
    pub fn acquire() -> Result<Self, String> {
        let name: Vec<u16> = INSTANCE_MUTEX_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(ptr::null_mut(), 1, name.as_ptr()) };
        if handle.is_null() {
            return Err("Failed to create EVOX2-Control instance mutex".to_string());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                ReleaseMutex(handle);
                CloseHandle(handle);
            }
            return Err(ALREADY_RUNNING.to_string());
        }
        Ok(Self { handle })
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

unsafe impl Send for InstanceGuard {}

pub fn gui_instance_running() -> bool {
    named_mutex_exists(INSTANCE_MUTEX_NAME)
}

fn named_mutex_exists(name: &str) -> bool {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { OpenMutexW(SYNCHRONIZE, 0, wide.as_ptr()) };
    if handle.is_null() {
        return false;
    }
    unsafe {
        CloseHandle(handle);
    }
    true
}

fn wide_name(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

/// GUI-owned event. CLI pulses it after writing fan curves to config.json.
pub struct ReloadFansGuard {
    handle: HANDLE,
}

impl ReloadFansGuard {
    pub fn create() -> Result<Self, String> {
        let name = wide_name(RELOAD_FANS_EVENT_NAME);
        let handle = unsafe { CreateEventW(ptr::null_mut(), 0, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(format!(
                "Failed to create reload-fans event (Win32 {})",
                unsafe { GetLastError() }
            ));
        }
        Ok(Self { handle })
    }

    pub fn poll(&self) -> bool {
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for ReloadFansGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

unsafe impl Send for ReloadFansGuard {}
unsafe impl Sync for ReloadFansGuard {}

pub fn signal_reload_fans() -> bool {
    let name = wide_name(RELOAD_FANS_EVENT_NAME);
    let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
    if handle.is_null() {
        return false;
    }
    let signaled = unsafe { SetEvent(handle) != 0 };
    unsafe {
        CloseHandle(handle);
    }
    signaled
}
unsafe impl Sync for InstanceGuard {}

pub fn current_exe_path() -> Result<String, String> {
    let mut buffer = [0u16; 260];
    let length =
        unsafe { GetModuleFileNameW(ptr::null_mut(), buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 {
        return Err("Failed to get executable path".to_string());
    }
    Ok(String::from_utf16_lossy(&buffer[..length as usize]))
}

pub fn is_start_with_windows() -> bool {
    if scheduled_task_registered() {
        return true;
    }
    let Ok(exe) = current_exe_path() else {
        return false;
    };
    for stored in [
        read_run_value(HKEY_CURRENT_USER),
        read_run_value(HKEY_LOCAL_MACHINE),
    ]
    .into_iter()
    .flatten()
    {
        if run_value_matches(&stored, &exe) {
            return true;
        }
    }
    false
}

pub fn set_start_with_windows(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = current_exe_path()?;
        let output = run_schtasks(&schtasks_create_args(&exe))?;
        if !output.success {
            return Err(format!(
                "Failed to register logon task ({})",
                output.message
            ));
        }
        let _ = remove_legacy_run_entries();
        if !scheduled_task_registered() {
            return Err("Logon task was not registered".to_string());
        }
        Ok(())
    } else {
        let output = run_schtasks(&schtasks_delete_args())?;
        remove_legacy_run_entries()?;
        let still_registered = scheduled_task_registered();
        if should_report_logon_remove_failure(output.success, still_registered) {
            if output.success {
                return Err("Logon task is still registered".to_string());
            }
            return Err(format!("Failed to remove logon task ({})", output.message));
        }
        Ok(())
    }
}

pub(crate) fn run_value_matches(stored: &str, exe_path: &str) -> bool {
    normalize_run_path(stored).eq_ignore_ascii_case(&normalize_run_path(exe_path))
}

pub(crate) fn registry_delete_succeeded(status: i32) -> bool {
    status == ERROR_SUCCESS as i32 || status == ERROR_FILE_NOT_FOUND as i32
}

pub(crate) fn schtasks_create_args(exe_path: &str) -> Vec<String> {
    vec![
        "/Create".into(),
        "/TN".into(),
        STARTUP_TASK_NAME.into(),
        "/TR".into(),
        format!("\"{exe_path}\""),
        "/SC".into(),
        "ONLOGON".into(),
        "/RL".into(),
        "HIGHEST".into(),
        "/F".into(),
    ]
}

pub(crate) fn schtasks_delete_args() -> Vec<String> {
    vec![
        "/Delete".into(),
        "/TN".into(),
        STARTUP_TASK_NAME.into(),
        "/F".into(),
    ]
}

pub(crate) fn schtasks_query_args() -> Vec<String> {
    vec!["/Query".into(), "/TN".into(), STARTUP_TASK_NAME.into()]
}

pub(crate) fn should_report_logon_remove_failure(_delete_ok: bool, still_registered: bool) -> bool {
    still_registered
}

pub(crate) fn decode_code_page(code_page: u32, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    unsafe {
        let needed = MultiByteToWideChar(
            code_page,
            0,
            bytes.as_ptr() as *const i8,
            bytes.len() as i32,
            ptr::null_mut(),
            0,
        );
        if needed <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut wide = vec![0u16; needed as usize];
        let written = MultiByteToWideChar(
            code_page,
            0,
            bytes.as_ptr() as *const i8,
            bytes.len() as i32,
            wide.as_mut_ptr(),
            needed,
        );
        if written <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        String::from_utf16_lossy(&wide[..written as usize])
    }
}

fn decode_console_bytes(bytes: &[u8]) -> String {
    decode_code_page(unsafe { GetACP() }, bytes)
}

#[cfg(test)]
pub(crate) fn schtasks_missing_task(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("cannot find")
        || lower.contains("the system cannot find")
        || lower.contains("does not exist")
        || lower.contains("cannot find the path")
        || message.contains("找不到")
        || message.contains("系统找不到")
        || message.contains("不存在")
}

fn normalize_run_path(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn scheduled_task_registered() -> bool {
    run_schtasks(&schtasks_query_args())
        .map(|output| output.success)
        .unwrap_or(false)
}

fn remove_legacy_run_entries() -> Result<(), String> {
    delete_run_value(HKEY_CURRENT_USER, KEY_SET_VALUE | KEY_READ)?;
    delete_run_value(
        HKEY_LOCAL_MACHINE,
        KEY_SET_VALUE | KEY_READ | KEY_WOW64_64KEY,
    )?;
    let _ = delete_named_value(
        HKEY_CURRENT_USER,
        STARTUP_APPROVED_KEY,
        RUN_VALUE,
        KEY_SET_VALUE | KEY_READ,
    );
    Ok(())
}

fn delete_run_value(root: HKEY, access: u32) -> Result<(), String> {
    delete_named_value(root, RUN_KEY, RUN_VALUE, access)
}

fn delete_named_value(root: HKEY, subkey: &str, name: &str, access: u32) -> Result<(), String> {
    let key_w = wide(subkey);
    let name_w = wide(name);
    let mut key = ptr::null_mut();
    let status = unsafe { RegOpenKeyExW(root, key_w.as_ptr(), 0, access, &mut key) };
    if status != ERROR_SUCCESS as i32 {
        if status == ERROR_FILE_NOT_FOUND as i32 {
            return Ok(());
        }
        return Err(format!("Failed to open startup registry key ({status})"));
    }
    let status = unsafe { RegDeleteValueW(key, name_w.as_ptr()) };
    unsafe { RegCloseKey(key) };
    if registry_delete_succeeded(status) {
        Ok(())
    } else {
        Err(format!(
            "Failed to remove startup registry value ({status})"
        ))
    }
}

fn read_run_value(root: HKEY) -> Option<String> {
    let key_w = wide(RUN_KEY);
    let name_w = wide(RUN_VALUE);
    let mut key = ptr::null_mut();
    let access = if std::ptr::eq(root, HKEY_LOCAL_MACHINE) {
        KEY_READ | KEY_WOW64_64KEY
    } else {
        KEY_READ
    };
    if unsafe { RegOpenKeyExW(root, key_w.as_ptr(), 0, access, &mut key) } != ERROR_SUCCESS as i32 {
        return None;
    }
    let mut kind: DWORD = 0;
    let mut size: DWORD = 0;
    if unsafe {
        RegQueryValueExW(
            key,
            name_w.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            ptr::null_mut(),
            &mut size,
        )
    } != ERROR_SUCCESS as i32
        || kind != REG_SZ
    {
        unsafe { RegCloseKey(key) };
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            name_w.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    unsafe { RegCloseKey(key) };
    if status != ERROR_SUCCESS as i32 {
        return None;
    }
    let words =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u16, (size as usize) / 2) };
    Some(normalize_run_path(
        String::from_utf16_lossy(words).trim_end_matches('\0'),
    ))
}

struct SchtasksOutput {
    success: bool,
    message: String,
}

fn run_schtasks(args: &[String]) -> Result<SchtasksOutput, String> {
    let exe = schtasks_exe();
    let output = Command::new(&exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Failed to run {exe}: {error}"))?;
    let mut message = decode_console_bytes(&output.stdout);
    let stderr = decode_console_bytes(&output.stderr);
    if !stderr.trim().is_empty() {
        if !message.trim().is_empty() {
            message.push('\n');
        }
        message.push_str(stderr.trim());
    }
    Ok(SchtasksOutput {
        success: output.status.success(),
        message: message.trim().to_string(),
    })
}

fn schtasks_exe() -> String {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    format!(r"{windir}\System32\schtasks.exe")
}

pub fn shell_open(path: &str) {
    let operation = wide("open");
    let path_w = wide(path);
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            path_w.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn pick_json_file(hwnd: isize, save: bool) -> Option<String> {
    unsafe {
        let mut file = [0u16; 1024];
        if save {
            copy_wide_buf(&mut file, "evox2-config.json");
        }
        let mut filter = json_file_filter();
        let mut title = wide(if save {
            "Export configuration"
        } else {
            "Import configuration"
        });
        let mut ofn: OPENFILENAMEW = mem::zeroed();
        ofn.lStructSize = mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = hwnd as HWND;
        ofn.lpstrFilter = filter.as_mut_ptr();
        ofn.nFilterIndex = 1;
        ofn.lpstrFile = file.as_mut_ptr();
        ofn.nMaxFile = file.len() as u32;
        ofn.lpstrTitle = title.as_mut_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;
        let ok = if save {
            ofn.Flags |= OFN_OVERWRITEPROMPT;
            GetSaveFileNameW(&mut ofn) != 0
        } else {
            ofn.Flags |= OFN_FILEMUSTEXIST;
            GetOpenFileNameW(&mut ofn) != 0
        };
        if !ok {
            return None;
        }
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        if len == 0 {
            None
        } else {
            Some(String::from_utf16_lossy(&file[..len]))
        }
    }
}

fn json_file_filter() -> Vec<u16> {
    let mut out = Vec::new();
    for part in ["JSON", "*.json", "All files", "*.*"] {
        out.extend(part.encode_utf16());
        out.push(0);
    }
    out.push(0);
    out
}

fn copy_wide_buf(dest: &mut [u16], text: &str) {
    let encoded: Vec<u16> = text.encode_utf16().collect();
    let len = encoded.len().min(dest.len().saturating_sub(1));
    dest[..len].copy_from_slice(&encoded[..len]);
    dest[len] = 0;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiChrome {
    pub dark: bool,
    pub accent: (u8, u8, u8),
}

pub fn ui_chrome() -> UiChrome {
    UiChrome {
        dark: !apps_use_light_theme(),
        accent: accent_color_rgb().unwrap_or((0, 120, 212)),
    }
}

pub fn apps_use_light_theme() -> bool {
    read_hkcu_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )
    .map(|value| value != 0)
    .unwrap_or(true)
}

pub fn accent_color_rgb() -> Option<(u8, u8, u8)> {
    read_hkcu_dword(r"Software\Microsoft\Windows\DWM", "AccentColor")
        .or_else(|| read_hkcu_dword(r"Software\Microsoft\Windows\DWM", "ColorizationColor"))
        .map(accent_from_abgr)
        .filter(|&(r, g, b)| r != 0 || g != 0 || b != 0)
}

pub(crate) fn accent_from_abgr(abgr: u32) -> (u8, u8, u8) {
    (
        (abgr & 0xFF) as u8,
        ((abgr >> 8) & 0xFF) as u8,
        ((abgr >> 16) & 0xFF) as u8,
    )
}

fn read_hkcu_dword(subkey: &str, value_name: &str) -> Option<u32> {
    unsafe {
        let subkey_w = wide(subkey);
        let mut key = ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ, &mut key)
            != ERROR_SUCCESS as i32
        {
            return None;
        }
        let name_w = wide(value_name);
        let mut kind: DWORD = 0;
        let mut data: DWORD = 0;
        let mut size = std::mem::size_of::<DWORD>() as DWORD;
        let status = RegQueryValueExW(
            key,
            name_w.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            &mut data as *mut DWORD as *mut u8,
            &mut size,
        );
        RegCloseKey(key);
        if status == ERROR_SUCCESS as i32 && kind == REG_DWORD {
            Some(data)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GBK_FILE_NOT_FOUND: &[u8] = &[
        0xB4, 0xED, 0xCE, 0xF3, 0x3A, 0x20, 0xCF, 0xB5, 0xCD, 0xB3, 0xD5, 0xD2, 0xB2, 0xBB, 0xB5,
        0xBD, 0xD6, 0xB8, 0xB6, 0xA8, 0xB5, 0xC4, 0xCE, 0xC4, 0xBC, 0xFE, 0xA1, 0xA3,
    ];

    #[test]
    fn run_value_matches_quoted_and_unquoted_paths() {
        let exe = r"C:\Program Files\ec-su_axb35-win\evox2-control.exe";
        assert!(run_value_matches(exe, exe));
        assert!(run_value_matches(&format!("\"{exe}\""), exe));
        assert!(run_value_matches(exe, &format!("\"{exe}\"")));
        assert!(!run_value_matches(
            r"C:\Program Files\other\evox2-control.exe",
            exe
        ));
    }

    #[test]
    fn registry_delete_treats_missing_value_as_success() {
        assert!(registry_delete_succeeded(ERROR_SUCCESS as i32));
        assert!(registry_delete_succeeded(ERROR_FILE_NOT_FOUND as i32));
        assert!(!registry_delete_succeeded(5));
    }

    #[test]
    fn logon_task_is_created_elevated() {
        let args = schtasks_create_args(r"C:\Program Files\ec-su_axb35-win\evox2-control.exe");
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "/SC" && pair[1] == "ONLOGON"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "/RL" && pair[1] == "HIGHEST"));
        assert!(args.windows(2).any(|pair| {
            pair[0] == "/TR" && pair[1] == r#""C:\Program Files\ec-su_axb35-win\evox2-control.exe""#
        }));
        assert!(args.contains(&"/F".to_string()));
    }

    #[test]
    fn missing_task_messages_are_recognized() {
        assert!(schtasks_missing_task(
            r#"ERROR: The specified task name "EVO-X2 Control" does not exist in the system."#
        ));
        assert!(schtasks_missing_task(
            "ERROR: The system cannot find the file specified."
        ));
        assert!(schtasks_missing_task("系统找不到指定的文件。"));
        assert!(schtasks_missing_task("指定的任务名不存在"));
        assert!(!schtasks_missing_task("Access is denied."));
    }

    #[test]
    fn gbk_missing_task_is_not_recognized_as_utf8() {
        let lossy = String::from_utf8_lossy(GBK_FILE_NOT_FOUND);
        assert!(
            !schtasks_missing_task(&lossy),
            "UTF-8 lossy of GBK schtasks text must not look like a missing-task message: {lossy:?}"
        );
    }

    #[test]
    fn gbk_code_page_recovers_missing_task_message() {
        let message = decode_code_page(936, GBK_FILE_NOT_FOUND);
        assert!(
            message.contains("找不到"),
            "ACP/GBK decode should recover Chinese schtasks text, got {message:?}"
        );
        assert!(schtasks_missing_task(&message));
    }

    #[test]
    fn absent_logon_task_is_not_a_remove_failure() {
        assert!(!should_report_logon_remove_failure(false, false));
        assert!(should_report_logon_remove_failure(false, true));
        assert!(!should_report_logon_remove_failure(true, false));
        assert!(should_report_logon_remove_failure(true, true));
    }

    #[test]
    fn accent_from_abgr_matches_windows_layout() {
        assert_eq!(accent_from_abgr(0xFF_D4_78_00), (0x00, 0x78, 0xD4));
        assert_eq!(accent_from_abgr(0x00_00_00_FF), (0xFF, 0x00, 0x00));
    }

    #[test]
    fn host_name_is_nonempty_on_windows() {
        let name = host_name();
        assert!(!name.is_empty());
        if let Some(computer) = env_computer_name() {
            assert!(
                name.eq_ignore_ascii_case(&computer)
                    || name.contains(&computer)
                    || computer.contains(&name),
                "host_name {name:?} should relate to COMPUTERNAME {computer:?}"
            );
        }
    }

    #[test]
    fn named_mutex_exists_tracks_create_and_close() {
        let name = format!("Local\\EVOX2-Control-Test-{}", std::process::id());
        assert!(!named_mutex_exists(&name));
        let wide = wide_name(&name);
        let handle = unsafe { CreateMutexW(ptr::null_mut(), 0, wide.as_ptr()) };
        assert!(!handle.is_null());
        assert!(named_mutex_exists(&name));
        unsafe {
            CloseHandle(handle);
        }
        assert!(!named_mutex_exists(&name));
    }
}
