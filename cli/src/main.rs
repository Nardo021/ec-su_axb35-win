use std::ptr;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use winapi::shared::minwindef::DWORD;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::winnt::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE, KEY_READ,
    REG_DWORD, REG_SZ, TOKEN_ELEVATION, TOKEN_QUERY,
};
use winapi::um::winreg::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE};

const DEFAULT_SERVER: &str = "http://127.0.0.1:8395";
const PAWNIO_WIN32_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";

#[derive(Parser, Debug)]
#[command(name = "evox2ctl")]
#[command(about = "EVO-X2 / SU_AXB35 control CLI (localhost REST API)")]
struct Args {
    #[arg(long, default_value = DEFAULT_SERVER)]
    server: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Get or set the APU power mode
    Mode {
        power_mode: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Show a short hardware/status summary
    Status,
    /// Report diagnostics without changing hardware state
    Diagnose,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    status: u8,
    version: Option<String>,
    pawnio: Option<String>,
    lpcacpiec: Option<String>,
    secure_boot: Option<String>,
    hardware: Option<String>,
    hardware_supported: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PowerModeResponse {
    power_mode: String,
}

#[derive(Debug, Deserialize)]
struct TemperatureResponse {
    temperature: u8,
}

#[derive(Debug, Deserialize)]
struct FanRpmResponse {
    rpm: u16,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    match args.command {
        Command::Mode {
            power_mode,
            dry_run,
        } => cmd_mode(&args.server, power_mode.as_deref(), dry_run),
        Command::Status => cmd_status(&args.server),
        Command::Diagnose => cmd_diagnose(&args.server),
    }
}

fn cmd_mode(server: &str, power_mode: Option<&str>, dry_run: bool) -> Result<(), String> {
    match power_mode {
        None => {
            let response: PowerModeResponse = get_json(server, "/apu/power_mode")?;
            println!("{}", response.power_mode);
            Ok(())
        }
        Some(mode) => {
            let (register, value, label) = match mode {
                "balanced" => (0x31u8, 0x00u8, "Balanced"),
                "performance" => (0x31, 0x01, "Performance"),
                "quiet" => (0x31, 0x02, "Quiet"),
                _ => {
                    return Err(format!(
                        "Invalid power mode '{mode}'. Use quiet, balanced, or performance."
                    ))
                }
            };

            if dry_run {
                println!("Would set EVO-X2 P-MODE:");
                println!("register: 0x{register:02X}");
                println!("value:    0x{value:02X}");
                println!("mode:     {label}");
                return Ok(());
            }

            let body = serde_json::json!({ "power_mode": mode });
            let response: PowerModeResponse = post_json(server, "/apu/power_mode", &body)?;
            println!("{}", response.power_mode);
            Ok(())
        }
    }
}

fn cmd_status(server: &str) -> Result<(), String> {
    let status = get_json::<StatusResponse>(server, "/status").ok();
    let power = get_json::<PowerModeResponse>(server, "/apu/power_mode")?;
    let temp = get_json::<TemperatureResponse>(server, "/apu/temp")?;
    let fan1 = get_json::<FanRpmResponse>(server, "/fan1/rpm")?;
    let fan2 = get_json::<FanRpmResponse>(server, "/fan2/rpm")?;
    let fan3 = get_json::<FanRpmResponse>(server, "/fan3/rpm")?;

    println!(
        "Model: {}",
        status
            .as_ref()
            .and_then(|s| s.hardware.clone())
            .unwrap_or_else(bios_summary)
    );
    println!("Power Mode: {}", title_case(&power.power_mode));
    println!("APU Temp: {} C", temp.temperature);
    println!("CPU Fan: {} RPM", fan1.rpm);
    println!("Secondary CPU/APU Fan: {} RPM", fan2.rpm);
    println!("System Fan: {} RPM", fan3.rpm);
    println!(
        "PawnIO: {}",
        status
            .as_ref()
            .and_then(|s| s.pawnio.clone())
            .map(title_case)
            .unwrap_or_else(|| if pawnio_device_present() {
                "Connected".to_string()
            } else {
                "Unavailable".to_string()
            })
    );
    println!(
        "Secure Boot: {}",
        status
            .as_ref()
            .and_then(|s| s.secure_boot.clone())
            .unwrap_or_else(secure_boot_status)
    );
    Ok(())
}

fn cmd_diagnose(server: &str) -> Result<(), String> {
    let service_status = get_json::<StatusResponse>(server, "/status").ok();
    let power = get_json::<PowerModeResponse>(server, "/apu/power_mode").ok();

    println!("Operating system: Windows ({})", std::env::consts::OS);
    println!("Architecture: {}", std::env::consts::ARCH);
    println!(
        "Administrator/service status: {}",
        if is_admin() {
            "Administrator"
        } else {
            "Standard user"
        }
    );
    println!(
        "PawnIO status: {}",
        if pawnio_device_present() {
            match pawnio_install_version() {
                Some(version) => format!("Connected (installed {version})"),
                None => "Connected".to_string(),
            }
        } else {
            "Unavailable — install the official release from https://pawnio.eu/".to_string()
        }
    );
    println!(
        "LpcACPIEC module status: {}",
        service_status
            .as_ref()
            .and_then(|s| s.lpcacpiec.clone())
            .unwrap_or_else(|| "unknown (service not reporting)".to_string())
    );
    println!("Detected motherboard/product: {}", bios_summary());
    println!(
        "EC firmware version: {}",
        service_status
            .as_ref()
            .and_then(|s| s.version.clone())
            .unwrap_or_else(|| "unknown (service not reachable)".to_string())
    );
    println!(
        "Current P-MODE: {}",
        power
            .map(|p| p.power_mode)
            .unwrap_or_else(|| "unknown (service not reachable)".to_string())
    );
    println!("Secure Boot status: {}", secure_boot_status());
    if let Some(status) = service_status {
        if status.status != 1 {
            println!("Service EC status: unavailable");
        }
        if status.hardware_supported == Some(false) {
            println!("Hardware support: unsupported — EC writes are disabled");
        }
    }
    Ok(())
}

fn get_json<T: for<'de> Deserialize<'de>>(server: &str, path: &str) -> Result<T, String> {
    let url = format!("{server}{path}");
    let response = reqwest::blocking::get(&url).map_err(|e| format!("Request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format_http_error(response));
    }
    response
        .json()
        .map_err(|e| format!("Failed to parse {path}: {e}"))
}

fn post_json<T: for<'de> Deserialize<'de>>(
    server: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<T, String> {
    let url = format!("{server}{path}");
    let response = reqwest::blocking::Client::new()
        .post(&url)
        .json(body)
        .send()
        .map_err(|e| format!("Request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format_http_error(response));
    }
    response
        .json()
        .map_err(|e| format!("Failed to parse {path}: {e}"))
}

fn format_http_error(response: reqwest::blocking::Response) -> String {
    let status = response.status();
    match response.json::<ErrorResponse>() {
        Ok(body) => body.error.unwrap_or_else(|| format!("HTTP {status}")),
        Err(_) => format!("HTTP {status}"),
    }
}

fn title_case(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn is_admin() -> bool {
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

fn pawnio_device_present() -> bool {
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

fn pawnio_install_version() -> Option<String> {
    read_reg_string(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO",
        "DisplayVersion",
    )
    .or_else(|| {
        read_reg_string(
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO",
            "DisplayVersion",
        )
    })
}

fn secure_boot_status() -> String {
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

fn bios_summary() -> String {
    let manufacturer = read_reg_string(r"HARDWARE\DESCRIPTION\System\BIOS", "SystemManufacturer")
        .unwrap_or_default();
    let product = read_reg_string(r"HARDWARE\DESCRIPTION\System\BIOS", "SystemProductName")
        .unwrap_or_default();
    let board = read_reg_string(r"HARDWARE\DESCRIPTION\System\BIOS", "BaseBoardProduct")
        .unwrap_or_default();
    let text = format!("{manufacturer} {product} {board}");
    let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

fn read_reg_string(subkey: &str, value_name: &str) -> Option<String> {
    unsafe {
        let subkey_w = wide(subkey);
        let mut key = ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey_w.as_ptr(), 0, KEY_READ, &mut key)
            != ERROR_SUCCESS as i32
        {
            return None;
        }
        let name_w = wide(value_name);
        let mut kind: DWORD = 0;
        let mut size: DWORD = 0;
        if RegQueryValueExW(
            key,
            name_w.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            ptr::null_mut(),
            &mut size,
        ) != ERROR_SUCCESS as i32
            || kind != REG_SZ
        {
            RegCloseKey(key);
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        let status = RegQueryValueExW(
            key,
            name_w.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut size,
        );
        RegCloseKey(key);
        if status != ERROR_SUCCESS as i32 {
            return None;
        }
        let words = std::slice::from_raw_parts(buffer.as_ptr() as *const u16, (size as usize) / 2);
        Some(
            String::from_utf16_lossy(words)
                .trim_end_matches('\0')
                .trim()
                .to_string(),
        )
    }
}

fn read_reg_dword(subkey: &str, value_name: &str) -> Option<u32> {
    unsafe {
        let subkey_w = wide(subkey);
        let mut key = ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey_w.as_ptr(), 0, KEY_READ, &mut key)
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

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
