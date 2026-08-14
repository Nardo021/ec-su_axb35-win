use winapi::shared::minwindef::{DWORD, HKEY};
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::winnt::{KEY_READ, REG_SZ};
use winapi::um::winreg::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE};

#[derive(Debug, Clone, Default)]
pub struct HardwareIdentity {
    pub system_manufacturer: String,
    pub system_product: String,
    pub board_manufacturer: String,
    pub board_product: String,
}

impl HardwareIdentity {
    pub fn detect() -> Self {
        Self {
            system_manufacturer: read_bios_string("SystemManufacturer"),
            system_product: read_bios_string("SystemProductName"),
            board_manufacturer: read_bios_string("BaseBoardManufacturer"),
            board_product: read_bios_string("BaseBoardProduct"),
        }
    }

    pub fn summary(&self) -> String {
        let product = first_non_empty(&[&self.system_product, &self.board_product]);
        let manufacturer = first_non_empty(&[&self.system_manufacturer, &self.board_manufacturer]);
        match (manufacturer, product) {
            ("", "") => "unknown".to_string(),
            (manufacturer, "") => manufacturer.to_string(),
            ("", product) => product.to_string(),
            (manufacturer, product) => format!("{manufacturer} {product}"),
        }
    }

    pub fn is_supported_axb35(&self) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.system_manufacturer,
            self.system_product,
            self.board_manufacturer,
            self.board_product
        )
        .to_ascii_lowercase();

        contains_token(&haystack, "axb35")
            || contains_token(&haystack, "su_axb35")
            || contains_token(&haystack, "evo-x2")
            || contains_token(&haystack, "evox2")
            || haystack.contains("sixunited")
    }
}

fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or("")
}

fn contains_token(haystack: &str, token: &str) -> bool {
    haystack.contains(token)
}

fn read_bios_string(value_name: &str) -> String {
    read_reg_string(
        HKEY_LOCAL_MACHINE,
        r"HARDWARE\DESCRIPTION\System\BIOS",
        value_name,
    )
    .unwrap_or_default()
}

pub fn read_reg_string(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    unsafe {
        let subkey_w = wide(subkey);
        let mut key = std::ptr::null_mut();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut key) != ERROR_SUCCESS as i32 {
            return None;
        }

        let name_w = wide(value_name);
        let mut kind: DWORD = 0;
        let mut size: DWORD = 0;
        let query = RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        );
        if query != ERROR_SUCCESS as i32 || kind != REG_SZ || size == 0 {
            RegCloseKey(key);
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        let query = RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr(),
            &mut size,
        );
        RegCloseKey(key);
        if query != ERROR_SUCCESS as i32 {
            return None;
        }

        let u16_len = (size as usize) / 2;
        let words = std::slice::from_raw_parts(buffer.as_ptr() as *const u16, u16_len);
        Some(
            String::from_utf16_lossy(words)
                .trim_end_matches('\0')
                .trim()
                .to_string(),
        )
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_evo_x2_and_axb35_strings() {
        let identity = HardwareIdentity {
            system_manufacturer: "GMKtec".into(),
            system_product: "EVO-X2".into(),
            board_manufacturer: "Sixunited".into(),
            board_product: "SU_AXB35".into(),
        };
        assert!(identity.is_supported_axb35());
        assert_eq!(identity.summary(), "GMKtec EVO-X2");
    }

    #[test]
    fn rejects_unrelated_machines() {
        let identity = HardwareIdentity {
            system_manufacturer: "Dell Inc.".into(),
            system_product: "OptiPlex 7090".into(),
            board_manufacturer: "Dell Inc.".into(),
            board_product: "0XYZ12".into(),
        };
        assert!(!identity.is_supported_axb35());
    }
}
