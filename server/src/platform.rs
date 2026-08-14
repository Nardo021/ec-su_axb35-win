use std::ptr;

use winapi::shared::minwindef::DWORD;
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::synchapi::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::um::winnt::{HANDLE, TOKEN_ELEVATION, TOKEN_QUERY};
use winapi::um::winreg::HKEY_LOCAL_MACHINE;

use crate::hardware::read_reg_string;

const WAIT_ABANDONED: DWORD = 0x00000080;
const ACCESS_EC_TIMEOUT_MS: DWORD = 5_000;

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
