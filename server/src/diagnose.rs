use serde::Serialize;

use crate::hardware::HardwareIdentity;
use crate::i18n::{t, Language};
use crate::pawnio;
use crate::platform::{is_admin, pawnio_install_version, secure_boot_status};
use crate::session::AppSession;

pub const REPO_URL: &str = "https://github.com/Nardo021/ec-su_axb35-win";

#[derive(Clone, Debug, Serialize)]
pub struct DiagnoseReport {
    pub app_version: String,
    pub os: String,
    pub architecture: String,
    pub admin: bool,
    pub pawnio: String,
    pub pawnio_version: Option<String>,
    pub lpcacpiec: String,
    pub hardware: String,
    pub hardware_supported: bool,
    pub firmware: Option<String>,
    pub power_mode: Option<String>,
    pub secure_boot: String,
    pub temperature: Option<u8>,
    pub temperature_source: Option<String>,
    pub ec_raw_temp: Option<u8>,
}

impl DiagnoseReport {
    pub fn collect(session: Option<&AppSession>) -> Self {
        let pawnio_present = pawnio::probe_device_present();
        let pawnio_version = pawnio_install_version();
        let identity = HardwareIdentity::detect();
        let hardware = session
            .map(|item| item.runtime.hardware.clone())
            .unwrap_or_else(|| identity.summary());
        let hardware_supported = session
            .map(|item| item.runtime.hardware_supported)
            .unwrap_or_else(|| identity.is_supported_axb35());
        let mut report = Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: format!("Windows ({})", std::env::consts::OS),
            architecture: std::env::consts::ARCH.to_string(),
            admin: is_admin(),
            pawnio: if pawnio_present {
                "connected".into()
            } else {
                "unavailable".into()
            },
            pawnio_version,
            lpcacpiec: session
                .map(|item| item.runtime.lpcacpiec.clone())
                .unwrap_or_else(|| "unknown".into()),
            hardware,
            hardware_supported,
            firmware: None,
            power_mode: None,
            secure_boot: session
                .map(|item| item.runtime.secure_boot.clone())
                .unwrap_or_else(secure_boot_status),
            temperature: None,
            temperature_source: None,
            ec_raw_temp: None,
        };
        if let Some(session) = session {
            report.firmware = session.firmware_version().ok();
            report.power_mode = session.power_mode().ok();
            if let Ok(ec_temp) = session.controller.read_ec_apu_temperature() {
                let (temp, source) = crate::thermal::resolve_with_source(ec_temp);
                report.temperature = Some(temp);
                report.temperature_source = Some(source.i18n_key().to_string());
                report.ec_raw_temp = Some(ec_temp);
            }
        }
        report
    }

    pub fn format_text(&self, language: Language) -> String {
        let mut lines = Vec::new();
        lines.push(format!("EVO-X2 Control {}", self.app_version));
        lines.push(format!("{} {}", t(language, "os"), self.os));
        lines.push(format!(
            "{} {}",
            t(language, "architecture"),
            self.architecture
        ));
        lines.push(format!(
            "{} {}",
            t(language, "admin_status"),
            if self.admin {
                t(language, "administrator")
            } else {
                t(language, "standard_user")
            }
        ));
        let pawnio = if self.pawnio == "connected" {
            match &self.pawnio_version {
                Some(version) => format!("{}{version})", t(language, "pawnio_connected_ver")),
                None => t(language, "pawnio_connected").to_string(),
            }
        } else {
            t(language, "pawnio_unavailable").to_string()
        };
        lines.push(format!("{} {}", t(language, "pawnio_status"), pawnio));
        lines.push(format!(
            "{} {}",
            t(language, "lpcacpiec_status"),
            if self.lpcacpiec == "unknown" {
                t(language, "unknown").to_string()
            } else {
                self.lpcacpiec.clone()
            }
        ));
        lines.push(format!("{} {}", t(language, "motherboard"), self.hardware));
        lines.push(format!(
            "{} {}",
            t(language, "firmware_status"),
            self.firmware
                .as_deref()
                .unwrap_or_else(|| t(language, "unknown"))
        ));
        lines.push(format!(
            "{} {}",
            t(language, "current_pmode"),
            self.power_mode
                .as_deref()
                .unwrap_or_else(|| t(language, "unknown"))
        ));
        lines.push(format!(
            "{} {}",
            t(language, "secure_boot_status"),
            self.secure_boot
        ));
        if let (Some(temp), Some(source)) = (self.temperature, self.temperature_source.as_deref()) {
            lines.push(format!(
                "{} {} C ({})",
                t(language, "apu_temp"),
                temp,
                t(language, source)
            ));
        }
        if let Some(ec_temp) = self.ec_raw_temp {
            lines.push(format!("{} {} C", t(language, "ec_raw_temp"), ec_temp));
        }
        if !self.hardware_supported {
            lines.push(t(language, "hardware_unsupported").to_string());
        }
        lines.push(format!("{} {REPO_URL}", t(language, "repository")));
        lines.join("\n")
    }
}
