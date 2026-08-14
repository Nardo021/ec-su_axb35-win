use std::ptr;

use winapi::shared::minwindef::DWORD;
use winapi::shared::winerror::ERROR_ALREADY_EXISTS;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::CloseHandle;
use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::synchapi::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::um::winnt::{HANDLE, KEY_READ, KEY_SET_VALUE, REG_SZ, TOKEN_ELEVATION, TOKEN_QUERY};
use winapi::um::winreg::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
};

use crate::hardware::read_reg_string;

const WAIT_ABANDONED: DWORD = 0x00000080;
const ACCESS_EC_TIMEOUT_MS: DWORD = 5_000;
pub const ALREADY_RUNNING: &str = "ALREADY_RUNNING";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "EVO-X2 Control";

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
        let name: Vec<u16> = "Global\\EVOX2-Control"
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
    let Some(stored) = read_run_value() else {
        return false;
    };
    current_exe_path()
        .map(|path| {
            stored.eq_ignore_ascii_case(&path)
                || stored.eq_ignore_ascii_case(&format!("\"{path}\""))
        })
        .unwrap_or(false)
}

pub fn set_start_with_windows(enabled: bool) -> Result<(), String> {
    let key_w = wide(RUN_KEY);
    let name_w = wide(RUN_VALUE);
    let mut key = ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_w.as_ptr(),
            0,
            KEY_SET_VALUE | KEY_READ,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS as i32 {
        return Err(format!("Failed to open HKCU Run key ({status})"));
    }
    let result = if enabled {
        let path = current_exe_path()?;
        let quoted = format!("\"{path}\"");
        let value = wide(&quoted);
        let bytes = (value.len() * 2) as DWORD;
        let status = unsafe {
            RegSetValueExW(
                key,
                name_w.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                bytes,
            )
        };
        if status == ERROR_SUCCESS as i32 {
            Ok(())
        } else {
            Err(format!("Failed to write HKCU Run value ({status})"))
        }
    } else {
        unsafe { RegDeleteValueW(key, name_w.as_ptr()) };
        Ok(())
    };
    unsafe { RegCloseKey(key) };
    result
}

fn read_run_value() -> Option<String> {
    let key_w = wide(RUN_KEY);
    let name_w = wide(RUN_VALUE);
    let mut key = ptr::null_mut();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key_w.as_ptr(), 0, KEY_READ, &mut key) }
        != ERROR_SUCCESS as i32
    {
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
    Some(
        String::from_utf16_lossy(words)
            .trim_end_matches('\0')
            .trim_matches('"')
            .to_string(),
    )
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
