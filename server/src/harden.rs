use std::path::{Component, Path, PathBuf};
use std::ptr;

use winapi::shared::minwindef::HMODULE;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::accctrl::SE_FILE_OBJECT;
use winapi::um::aclapi::SetNamedSecurityInfoW;
use winapi::um::libloaderapi::LoadLibraryW;
use winapi::um::securitybaseapi::GetSecurityDescriptorDacl;
use winapi::um::shlobj::{SHGetFolderPathW, CSIDL_PROGRAM_FILES, CSIDL_PROGRAM_FILESX86};
use winapi::um::sysinfoapi::{GetSystemDirectoryW, GetWindowsDirectoryW};
use winapi::um::winbase::LocalFree;
use winapi::um::winnt::{
    DACL_SECURITY_INFORMATION, PACL, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
};

const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
const SDDL_REVISION_1: u32 = 1;
const PROGRAM_DATA_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

#[link(name = "kernel32")]
extern "system" {
    fn SetDefaultDllDirectories(directory_flags: u32) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        string_security_descriptor: *const u16,
        string_sd_revision: u32,
        security_descriptor: *mut PSECURITY_DESCRIPTOR,
        security_descriptor_size: *mut u32,
    ) -> i32;
}

pub fn restrict_dll_search() {
    unsafe {
        let _ = SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32);
    }
}

pub fn system_directory() -> String {
    query_windows_dir(GetSystemDirectoryW, r"C:\Windows\System32")
}

pub fn windows_directory() -> String {
    query_windows_dir(GetWindowsDirectoryW, r"C:\Windows")
}

pub fn system32_library_path(file_name: &str) -> Option<String> {
    if !is_safe_library_file_name(file_name) {
        return None;
    }
    Some(format!("{}\\{file_name}", system_directory()))
}

pub fn load_system32_library(file_name: &str) -> HMODULE {
    let Some(path) = system32_library_path(file_name) else {
        return ptr::null_mut();
    };
    let wide = wide(&path);
    unsafe { LoadLibraryW(wide.as_ptr()) }
}

pub fn harden_program_data_dir(path: &str) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|error| format!("Failed to create config directory: {error}"))?;
    apply_admin_only_dacl(path)
}

pub fn normalize_exe_path(value: &str) -> String {
    let trimmed = value.trim().trim_matches('"').replace('/', "\\");
    let mut out = PathBuf::new();
    for component in Path::new(&trimmed).components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().replace('/', "\\")
}

pub fn path_is_under_program_files(exe: &str) -> bool {
    path_is_under_prefixes(exe, &program_files_dirs())
}

pub fn path_is_under_prefixes(exe: &str, prefixes: &[String]) -> bool {
    let normalized = normalize_exe_path(exe);
    if normalized.is_empty() {
        return false;
    }
    prefixes.iter().any(|prefix| {
        let prefix = normalize_exe_path(prefix)
            .trim_end_matches('\\')
            .to_string();
        if prefix.is_empty() {
            return false;
        }
        if normalized.eq_ignore_ascii_case(&prefix) {
            return true;
        }
        let with_sep = format!("{prefix}\\");
        normalized.len() > with_sep.len()
            && normalized[..with_sep.len()].eq_ignore_ascii_case(&with_sep)
    })
}

pub fn program_files_dirs() -> Vec<String> {
    [CSIDL_PROGRAM_FILES, CSIDL_PROGRAM_FILESX86]
        .into_iter()
        .filter_map(known_folder)
        .collect()
}

fn is_safe_library_file_name(file_name: &str) -> bool {
    !file_name.is_empty()
        && !file_name.contains(['\\', '/'])
        && !file_name.contains("..")
        && file_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn query_windows_dir(
    getter: unsafe extern "system" fn(*mut u16, u32) -> u32,
    fallback: &str,
) -> String {
    unsafe {
        let needed = getter(ptr::null_mut(), 0);
        let cap = if needed == 0 { 260 } else { needed };
        let mut buffer = vec![0u16; cap as usize];
        let written = getter(buffer.as_mut_ptr(), buffer.len() as u32);
        if written == 0 {
            return fallback.to_string();
        }
        let text = String::from_utf16_lossy(&buffer[..written as usize]);
        let text = text.trim_end_matches('\0').trim_end_matches('\\');
        if text.is_empty() {
            fallback.to_string()
        } else {
            text.to_string()
        }
    }
}

fn known_folder(csidl: i32) -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 260];
        let status = SHGetFolderPathW(
            ptr::null_mut(),
            csidl,
            ptr::null_mut(),
            0,
            buffer.as_mut_ptr(),
        );
        if status < 0 {
            return None;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let path = String::from_utf16_lossy(&buffer[..len]);
        let path = path.trim_end_matches('\\');
        if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        }
    }
}

fn apply_admin_only_dacl(path: &str) -> Result<(), String> {
    unsafe {
        let sddl = wide(PROGRAM_DATA_SDDL);
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err("Failed to build ProgramData security descriptor".to_string());
        }

        let mut dacl_present = 0i32;
        let mut dacl_defaulted = 0i32;
        let mut dacl: PACL = ptr::null_mut();
        let ok = GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        );
        if ok == 0 || dacl_present == 0 || dacl.is_null() {
            LocalFree(descriptor as *mut _);
            return Err("Failed to read ProgramData DACL".to_string());
        }

        let mut path_w = wide(path);
        let status = SetNamedSecurityInfoW(
            path_w.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        );
        LocalFree(descriptor as *mut _);
        if status != ERROR_SUCCESS {
            return Err(format!("Failed to harden ProgramData ACL ({status})"));
        }
        Ok(())
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system32_library_path_accepts_adl_name() {
        let path = system32_library_path("atiadlxx.dll").expect("safe name");
        assert!(
            path.to_ascii_lowercase()
                .ends_with(r"\system32\atiadlxx.dll"),
            "{path}"
        );
    }

    #[test]
    fn system32_library_path_rejects_traversal() {
        assert_eq!(system32_library_path(r"..\evil.dll"), None);
        assert_eq!(system32_library_path(r"foo\bar.dll"), None);
        assert_eq!(system32_library_path(""), None);
        assert_eq!(system32_library_path("atiadlxx.dll.exe?"), None);
    }

    #[test]
    fn normalize_exe_path_collapses_parent_dirs() {
        assert_eq!(
            normalize_exe_path(r"C:\Program Files\ec-su_axb35-win\..\..\Windows\System32\cmd.exe"),
            r"C:\Windows\System32\cmd.exe"
        );
        assert_eq!(
            normalize_exe_path(r#""C:\Program Files\ec-su_axb35-win\evox2-control.exe""#),
            r"C:\Program Files\ec-su_axb35-win\evox2-control.exe"
        );
    }

    #[test]
    fn program_files_prefix_matches_install_dir_only() {
        let prefixes = vec![
            r"C:\Program Files".into(),
            r"C:\Program Files (x86)".into(),
        ];
        assert!(path_is_under_prefixes(
            r"C:\Program Files\ec-su_axb35-win\evox2-control.exe",
            &prefixes
        ));
        assert!(path_is_under_prefixes(
            r"C:\Program Files (x86)\ec-su_axb35-win\evox2-control.exe",
            &prefixes
        ));
        assert!(!path_is_under_prefixes(
            r"C:\Program Files\ec-su_axb35-win\..\..\Windows\System32\cmd.exe",
            &prefixes
        ));
        assert!(!path_is_under_prefixes(
            r"D:\Dev\projects\ec-su_axb35-win\dist\evox2-control.exe",
            &prefixes
        ));
        assert!(!path_is_under_prefixes(
            r"C:\Program Files foo\evox2-control.exe",
            &prefixes
        ));
    }

    #[test]
    fn system_and_windows_directories_are_nonempty() {
        assert!(!system_directory().is_empty());
        assert!(!windows_directory().is_empty());
    }
}
