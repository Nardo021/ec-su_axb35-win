//! Official PawnIO userspace protocol (device IOCTL interface).
//!
//! Constants and buffer layout come from the official PawnIO sources:
//! - `PawnIO/include/pawnio_um.h` (`k_device_type`, `IOCTL_PIO_*`)
//! - `PawnIOLib/PawnIOLib.cpp` (`pawnio_open` / `pawnio_load` / `pawnio_execute`)
//! - `PawnIO.Modules/LpcACPIEC.p` (`ioctl_pio_read`, `ioctl_pio_write`)
//!
//! This crate talks to the already-installed signed PawnIO driver. It does not
//! ship or load a kernel `.sys` file.

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use winapi::shared::minwindef::{DWORD, FALSE};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::ioapiset::DeviceIoControl;
use winapi::um::winnt::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE, HANDLE,
};

use crate::ec_io::{is_acpi_ec_port, EcIoBackend};

/// Official NT device name is `\Device\PawnIO`. Win32 CreateFile opens it via GLOBALROOT.
const PAWNIO_WIN32_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";

/// From `pawnio_um.h`: `constexpr static ULONG k_device_type = 41394;`
const PIO_DEVICE_TYPE: u32 = 41394;
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

const fn ctl_code(function: u32) -> u32 {
    (PIO_DEVICE_TYPE << 16) | (FILE_ANY_ACCESS << 14) | (function << 2) | METHOD_BUFFERED
}

/// `IOCTL_PIO_LOAD_BINARY = CTL_CODE(k_device_type, 0x821, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_PIO_LOAD_BINARY: u32 = ctl_code(0x821);
/// `IOCTL_PIO_EXECUTE_FN = CTL_CODE(k_device_type, 0x841, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_PIO_EXECUTE_FN: u32 = ctl_code(0x841);
/// `IOCTL_PIO_VERSION = CTL_CODE(k_device_type, 0x861, METHOD_BUFFERED, FILE_ANY_ACCESS)`
const IOCTL_PIO_VERSION: u32 = ctl_code(0x861);

/// Official `pawnio_execute` function-name field width.
const FN_NAME_LENGTH: usize = 32;

/// Official LpcACPIEC functions from `LpcACPIEC.p`.
const FN_PIO_READ: &str = "ioctl_pio_read";
const FN_PIO_WRITE: &str = "ioctl_pio_write";

pub const PAWNIO_MISSING_MESSAGE: &str = "\
PawnIO is required for hardware access.

Secure Boot can remain enabled.

Install the official PawnIO release and restart this application.";

const LPCACPIEC_BIN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vendor/pawnio/LpcACPIEC.bin"
));

static PAWNIO_CONNECTED: AtomicBool = AtomicBool::new(false);
static LPCACPIEC_LOADED: AtomicBool = AtomicBool::new(false);

pub fn pawnio_connected() -> bool {
    PAWNIO_CONNECTED.load(Ordering::SeqCst)
}

pub fn lpcacpiec_loaded() -> bool {
    LPCACPIEC_LOADED.load(Ordering::SeqCst)
}

pub struct PawnIoBackend {
    handle: HANDLE,
}

unsafe impl Send for PawnIoBackend {}
unsafe impl Sync for PawnIoBackend {}

impl PawnIoBackend {
    pub fn connect() -> Result<Self, String> {
        let handle = open_pawnio()?;
        PAWNIO_CONNECTED.store(true, Ordering::SeqCst);

        load_binary(handle, LPCACPIEC_BIN).map_err(|e| {
            unsafe { CloseHandle(handle) };
            PAWNIO_CONNECTED.store(false, Ordering::SeqCst);
            format!("Failed to load official LpcACPIEC module: {e}")
        })?;
        LPCACPIEC_LOADED.store(true, Ordering::SeqCst);

        Ok(Self { handle })
    }

    pub fn driver_version(&self) -> Result<u32, String> {
        let mut version: u32 = 0;
        let mut bytes_returned: DWORD = 0;
        let success = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_PIO_VERSION,
                ptr::null_mut(),
                0,
                &mut version as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
                &mut bytes_returned,
                ptr::null_mut(),
            )
        };
        if success == FALSE {
            return Err(format!("IOCTL_PIO_VERSION failed (Win32 {})", unsafe {
                GetLastError()
            }));
        }
        Ok(version)
    }

    fn execute(&self, name: &str, input: &[u64], out_len: usize) -> Result<Vec<u64>, String> {
        if name.len() >= FN_NAME_LENGTH {
            return Err(format!("PawnIO function name too long: {name}"));
        }

        let mut payload = vec![0u8; FN_NAME_LENGTH + input.len() * 8];
        payload[..name.len()].copy_from_slice(name.as_bytes());
        for (index, value) in input.iter().enumerate() {
            let start = FN_NAME_LENGTH + index * 8;
            payload[start..start + 8].copy_from_slice(&value.to_le_bytes());
        }

        let mut output = vec![0u64; out_len];
        let mut bytes_returned: DWORD = 0;
        let out_ptr = if out_len == 0 {
            ptr::null_mut()
        } else {
            output.as_mut_ptr() as *mut _
        };
        let out_size = (out_len * 8) as u32;

        let success = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_PIO_EXECUTE_FN,
                payload.as_ptr() as *mut _,
                payload.len() as u32,
                out_ptr,
                out_size,
                &mut bytes_returned,
                ptr::null_mut(),
            )
        };

        if success == FALSE {
            return Err(format!("PawnIO {name} failed (Win32 {})", unsafe {
                GetLastError()
            }));
        }

        let returned = (bytes_returned as usize) / 8;
        output.truncate(returned.min(out_len));
        Ok(output)
    }
}

impl EcIoBackend for PawnIoBackend {
    fn read_port_u8(&self, port: u16) -> Result<u8, String> {
        if !is_acpi_ec_port(port) {
            return Err(format!(
                "Refusing PawnIO read of non-ACPI EC port 0x{port:02X}"
            ));
        }
        let out = self.execute(FN_PIO_READ, &[u64::from(port)], 1)?;
        out.first()
            .map(|value| *value as u8)
            .ok_or_else(|| "LpcACPIEC ioctl_pio_read returned no data".to_string())
    }

    fn write_port_u8(&self, port: u16, value: u8) -> Result<(), String> {
        if !is_acpi_ec_port(port) {
            return Err(format!(
                "Refusing PawnIO write of non-ACPI EC port 0x{port:02X}"
            ));
        }
        self.execute(FN_PIO_WRITE, &[u64::from(port), u64::from(value)], 0)?;
        Ok(())
    }
}

impl Drop for PawnIoBackend {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.handle);
            }
            self.handle = INVALID_HANDLE_VALUE;
        }
        LPCACPIEC_LOADED.store(false, Ordering::SeqCst);
        PAWNIO_CONNECTED.store(false, Ordering::SeqCst);
    }
}

fn open_pawnio() -> Result<HANDLE, String> {
    let path: Vec<u16> = PAWNIO_WIN32_PATH
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        return Err(format!(
            "{PAWNIO_MISSING_MESSAGE}\n\nCreateFile({PAWNIO_WIN32_PATH}) failed with Win32 {error}."
        ));
    }
    Ok(handle)
}

fn load_binary(handle: HANDLE, blob: &[u8]) -> Result<(), String> {
    let mut bytes_returned: DWORD = 0;
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_PIO_LOAD_BINARY,
            blob.as_ptr() as *mut _,
            blob.len() as u32,
            ptr::null_mut(),
            0,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    };
    if success == FALSE {
        return Err(format!("IOCTL_PIO_LOAD_BINARY failed (Win32 {})", unsafe {
            GetLastError()
        }));
    }
    Ok(())
}

pub fn probe_device_present() -> bool {
    let path: Vec<u16> = PAWNIO_WIN32_PATH
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return false;
    }
    unsafe {
        CloseHandle(handle);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_ioctl_codes_match_pawnio_um_h() {
        assert_eq!(IOCTL_PIO_LOAD_BINARY, (41394u32 << 16) | (0x821 << 2));
        assert_eq!(IOCTL_PIO_EXECUTE_FN, (41394u32 << 16) | (0x841 << 2));
        assert_eq!(IOCTL_PIO_VERSION, (41394u32 << 16) | (0x861 << 2));
    }

    #[test]
    fn execute_payload_lays_out_name_then_u64_cells() {
        let name = FN_PIO_READ;
        let input = [0x66u64];
        let mut payload = vec![0u8; FN_NAME_LENGTH + input.len() * 8];
        payload[..name.len()].copy_from_slice(name.as_bytes());
        payload[FN_NAME_LENGTH..FN_NAME_LENGTH + 8].copy_from_slice(&input[0].to_le_bytes());
        assert_eq!(&payload[..14], b"ioctl_pio_read");
        assert_eq!(payload[14], 0);
        assert_eq!(&payload[32..40], &0x66u64.to_le_bytes());
    }

    #[test]
    fn official_lpcacpiec_blob_is_present() {
        assert_eq!(LPCACPIEC_BIN.len(), 2612);
        assert!(LPCACPIEC_BIN.iter().any(|byte| *byte != 0));
    }
}
