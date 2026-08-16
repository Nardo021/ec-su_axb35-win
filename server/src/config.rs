use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::alert::{clamp_threshold, TEMP_ALERT_DEFAULT};
use crate::fan::{self, sanitize_name};
use crate::i18n::Language;
use crate::thermal::TemperatureSource;

pub const SMOOTHING_WINDOW_DEFAULT: u8 = 8;
pub const SMOOTHING_WINDOW_MIN: u8 = 1;
pub const SMOOTHING_WINDOW_MAX: u8 = 20;

pub fn clamp_smoothing_window(value: u8) -> u8 {
    value.clamp(SMOOTHING_WINDOW_MIN, SMOOTHING_WINDOW_MAX)
}

fn default_smoothing_window() -> u8 {
    SMOOTHING_WINDOW_DEFAULT
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FanConfig {
    pub mode: String,
    pub level: u8,
    pub rampup_curve: [u8; 5],
    pub rampdown_curve: [u8; 5],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Default for FanConfig {
    fn default() -> Self {
        Self::default_for_fan(1)
    }
}

impl FanConfig {
    pub fn default_for_fan(fan_id: u8) -> Self {
        match fan_id {
            3 => FanConfig {
                mode: "auto".to_string(),
                level: 0,
                rampup_curve: [20, 60, 83, 95, 97],
                rampdown_curve: [0, 50, 80, 94, 96],
                name: None,
            },
            _ => FanConfig {
                mode: "auto".to_string(),
                level: 0,
                rampup_curve: [60, 70, 83, 95, 97],
                rampdown_curve: [40, 50, 80, 94, 96],
                name: None,
            },
        }
    }
}

fn default_close_to_tray() -> bool {
    true
}

fn default_language() -> String {
    "en".to_string()
}

fn default_temp_alert_enabled() -> bool {
    true
}

fn default_temp_alert_celsius() -> u8 {
    TEMP_ALERT_DEFAULT
}

fn default_temperature_source() -> String {
    TemperatureSource::Gpu.code().to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub log_path: String,
    pub apu_power_mode: Option<String>,
    pub fan1: Option<FanConfig>,
    pub fan2: Option<FanConfig>,
    pub fan3: Option<FanConfig>,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_temp_alert_enabled")]
    pub temp_alert_enabled: bool,
    #[serde(default = "default_temp_alert_celsius")]
    pub temp_alert_celsius: u8,
    #[serde(default = "default_temperature_source")]
    pub temperature_source: String,
    #[serde(default = "default_smoothing_window")]
    pub smoothing_window: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processor_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PortableConfig {
    pub apu_power_mode: Option<String>,
    pub fan1: FanConfig,
    pub fan2: FanConfig,
    pub fan3: FanConfig,
    pub close_to_tray: bool,
    pub start_with_windows: bool,
    pub language: String,
    pub temp_alert_enabled: bool,
    pub temp_alert_celsius: u8,
    #[serde(default = "default_temperature_source")]
    pub temperature_source: String,
    #[serde(default = "default_smoothing_window")]
    pub smoothing_window: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processor_name: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8395,
            log_path: format!("{}\\server.log", program_data_dir()),
            apu_power_mode: None,
            fan1: Some(FanConfig::default_for_fan(1)),
            fan2: Some(FanConfig::default_for_fan(2)),
            fan3: Some(FanConfig::default_for_fan(3)),
            close_to_tray: true,
            start_with_windows: false,
            language: default_language(),
            temp_alert_enabled: default_temp_alert_enabled(),
            temp_alert_celsius: default_temp_alert_celsius(),
            temperature_source: default_temperature_source(),
            smoothing_window: default_smoothing_window(),
            processor_name: None,
        }
    }
}

pub fn program_data_dir() -> String {
    let system_drive = std::env::var("SYSTEMDRIVE").unwrap_or_else(|_| "C:".to_string());
    format!("{system_drive}\\ProgramData\\ec-su_axb35-win")
}

pub fn config_file_path() -> String {
    format!("{}\\config.json", program_data_dir())
}

impl ServerConfig {
    pub fn language(&self) -> Language {
        Language::from_code(&self.language)
    }

    pub fn temperature_source(&self) -> TemperatureSource {
        TemperatureSource::from_code(&self.temperature_source)
    }

    pub fn processor_custom_name(&self) -> Option<String> {
        self.processor_name.as_deref().and_then(sanitize_name)
    }

    pub fn fan_custom_name(&self, fan_id: u8) -> Option<String> {
        let fan = match fan_id {
            1 => self.fan1.as_ref(),
            2 => self.fan2.as_ref(),
            3 => self.fan3.as_ref(),
            _ => None,
        }?;
        fan.name.as_deref().and_then(sanitize_name)
    }

    pub fn load() -> Result<Self, String> {
        let config_path = config_file_path();

        if !Path::new(&config_path).exists() {
            let default_config = ServerConfig::default();
            let config_dir = Path::new(&config_path).parent().unwrap();
            if !config_dir.exists() {
                fs::create_dir_all(config_dir)
                    .map_err(|e| format!("Failed to create config directory: {}", e))?;
            }
            let config_json = serde_json::to_string_pretty(&default_config)
                .map_err(|e| format!("Failed to serialize default config: {}", e))?;
            fs::write(&config_path, config_json)
                .map_err(|e| format!("Failed to write default config: {}", e))?;
            return Ok(default_config);
        }

        let config_content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config file {}: {}", config_path, e))?;

        let mut config: ServerConfig = serde_json::from_str(&config_content)
            .map_err(|e| format!("Failed to parse config file: {}", e))?;

        if !config.log_path.contains(':') {
            config.log_path = format!("{}\\{}", program_data_dir(), config.log_path);
        }
        config.temp_alert_celsius = clamp_threshold(config.temp_alert_celsius);
        config.temperature_source = config.temperature_source().code().to_string();
        config.smoothing_window = clamp_smoothing_window(config.smoothing_window);
        sanitize_fan_names(&mut config);
        config.processor_name = config.processor_name.as_deref().and_then(sanitize_name);

        Ok(config)
    }

    pub fn save(&self) -> Result<(), String> {
        let config_path = config_file_path();
        let config_dir = Path::new(&config_path).parent().unwrap();
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let config_json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&config_path, config_json)
            .map_err(|e| format!("Failed to write config file: {}", e))?;
        Ok(())
    }

    pub fn to_portable(&self) -> PortableConfig {
        PortableConfig {
            apu_power_mode: self.apu_power_mode.clone(),
            fan1: self.fan1.clone().unwrap_or_default(),
            fan2: self.fan2.clone().unwrap_or_default(),
            fan3: self.fan3.clone().unwrap_or_default(),
            close_to_tray: self.close_to_tray,
            start_with_windows: self.start_with_windows,
            language: self.language.clone(),
            temp_alert_enabled: self.temp_alert_enabled,
            temp_alert_celsius: clamp_threshold(self.temp_alert_celsius),
            temperature_source: self.temperature_source().code().to_string(),
            smoothing_window: clamp_smoothing_window(self.smoothing_window),
            processor_name: self.processor_name.as_deref().and_then(sanitize_name),
        }
    }

    pub fn apply_portable(&mut self, portable: PortableConfig) {
        self.apu_power_mode = portable.apu_power_mode;
        self.fan1 = Some(portable.fan1);
        self.fan2 = Some(portable.fan2);
        self.fan3 = Some(portable.fan3);
        if let Some(fan) = self.fan1.as_mut() {
            sanitize_fan_config(fan);
        }
        if let Some(fan) = self.fan2.as_mut() {
            sanitize_fan_config(fan);
        }
        if let Some(fan) = self.fan3.as_mut() {
            sanitize_fan_config(fan);
        }
        self.close_to_tray = portable.close_to_tray;
        self.start_with_windows = portable.start_with_windows;
        self.language = portable.language;
        self.temp_alert_enabled = portable.temp_alert_enabled;
        self.temp_alert_celsius = clamp_threshold(portable.temp_alert_celsius);
        self.temperature_source = TemperatureSource::from_code(&portable.temperature_source)
            .code()
            .to_string();
        self.smoothing_window = clamp_smoothing_window(portable.smoothing_window);
        self.processor_name = portable.processor_name.as_deref().and_then(sanitize_name);
    }
}

impl PortableConfig {
    pub fn parse_json(json: &str) -> Result<Self, String> {
        let portable: PortableConfig = serde_json::from_str(json)
            .map_err(|error| format!("Invalid configuration file: {error}"))?;
        portable.validate()?;
        Ok(portable)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(mode) = &self.apu_power_mode {
            if !matches!(mode.as_str(), "quiet" | "balanced" | "performance") {
                return Err(format!("Invalid power mode: {mode}"));
            }
        }
        validate_fan("fan1", &self.fan1)?;
        validate_fan("fan2", &self.fan2)?;
        validate_fan("fan3", &self.fan3)?;
        if !matches!(self.language.as_str(), "zh" | "en") {
            return Err(format!("Invalid language: {}", self.language));
        }
        if !(70..=97).contains(&self.temp_alert_celsius) {
            return Err(format!(
                "Temperature alert threshold must be between 70 and 97, got {}",
                self.temp_alert_celsius
            ));
        }
        if TemperatureSource::from_code(&self.temperature_source).code() != self.temperature_source
        {
            return Err(format!(
                "Invalid temperature source: {}",
                self.temperature_source
            ));
        }
        if !(SMOOTHING_WINDOW_MIN..=SMOOTHING_WINDOW_MAX).contains(&self.smoothing_window) {
            return Err(format!(
                "Smoothing window must be between {SMOOTHING_WINDOW_MIN} and {SMOOTHING_WINDOW_MAX}, got {}",
                self.smoothing_window
            ));
        }
        if let Some(label) = &self.processor_name {
            if label.chars().count() > fan::NAME_MAX_CHARS * 2 {
                return Err("Invalid processor name: too long".to_string());
            }
        }
        Ok(())
    }
}

fn validate_fan(name: &str, fan: &FanConfig) -> Result<(), String> {
    if !matches!(fan.mode.as_str(), "auto" | "fixed" | "curve") {
        return Err(format!("Invalid {name} mode: {}", fan.mode));
    }
    if fan.level > 5 {
        return Err(format!("Invalid {name} level: {}", fan.level));
    }
    if let Some(label) = &fan.name {
        if label.chars().count() > fan::NAME_MAX_CHARS * 2 {
            return Err(format!("Invalid {name} name: too long"));
        }
    }
    Ok(())
}

fn sanitize_fan_config(fan: &mut FanConfig) {
    fan.name = fan.name.as_deref().and_then(sanitize_name);
}

fn sanitize_fan_names(config: &mut ServerConfig) {
    if let Some(fan) = config.fan1.as_mut() {
        sanitize_fan_config(fan);
    }
    if let Some(fan) = config.fan2.as_mut() {
        sanitize_fan_config(fan);
    }
    if let Some(fan) = config.fan3.as_mut() {
        sanitize_fan_config(fan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_fan_matches_stock_curves() {
        let fan1 = FanConfig::default_for_fan(1);
        let fan2 = FanConfig::default_for_fan(2);
        let fan3 = FanConfig::default_for_fan(3);
        assert_eq!(fan1.rampup_curve, [60, 70, 83, 95, 97]);
        assert_eq!(fan1.rampdown_curve, [40, 50, 80, 94, 96]);
        assert_eq!(fan2.rampup_curve, fan1.rampup_curve);
        assert_eq!(fan2.rampdown_curve, fan1.rampdown_curve);
        assert_eq!(fan3.rampup_curve, [20, 60, 83, 95, 97]);
        assert_eq!(fan3.rampdown_curve, [0, 50, 80, 94, 96]);
        assert_eq!(FanConfig::default(), fan1);
    }

    #[test]
    fn old_config_gets_ui_defaults() {
        let parsed: ServerConfig = serde_json::from_str(
            r#"{
                "host": "127.0.0.1",
                "port": 8395,
                "log_path": "C:\\ProgramData\\ec-su_axb35-win\\server.log"
            }"#,
        )
        .unwrap();
        assert!(parsed.close_to_tray);
        assert!(!parsed.start_with_windows);
        assert_eq!(parsed.language, "en");
        assert_eq!(parsed.language(), Language::En);
        assert!(parsed.temp_alert_enabled);
        assert_eq!(parsed.temp_alert_celsius, 90);
        assert_eq!(parsed.temperature_source, "gpu");
        assert_eq!(parsed.temperature_source(), TemperatureSource::Gpu);
        assert_eq!(parsed.smoothing_window, SMOOTHING_WINDOW_DEFAULT);
        assert_eq!(parsed.processor_name, None);
    }

    #[test]
    fn language_en_parses() {
        let parsed: ServerConfig = serde_json::from_str(
            r#"{
                "host": "127.0.0.1",
                "port": 8395,
                "log_path": "C:\\temp\\server.log",
                "close_to_tray": false,
                "start_with_windows": true,
                "language": "en"
            }"#,
        )
        .unwrap();
        assert!(!parsed.close_to_tray);
        assert!(parsed.start_with_windows);
        assert_eq!(parsed.language(), Language::En);
    }

    #[test]
    fn portable_export_omits_host_and_port() {
        let json = serde_json::to_string(&ServerConfig::default().to_portable()).unwrap();
        assert!(!json.contains("host"));
        assert!(!json.contains("port"));
        assert!(!json.contains("log_path"));
        assert!(json.contains("temp_alert_enabled"));
        assert!(json.contains("\"temperature_source\":\"gpu\""));
        assert!(json.contains("\"smoothing_window\":8"));
    }

    #[test]
    fn portable_import_rejects_missing_fields() {
        let error = PortableConfig::parse_json(r#"{"language":"zh"}"#).unwrap_err();
        assert!(error.contains("Invalid configuration file"));
    }

    #[test]
    fn portable_import_rejects_short_curve() {
        let json = r#"{
            "fan1": {"mode":"auto","level":0,"rampup_curve":[1,2,3],"rampdown_curve":[1,2,3,4,5]},
            "fan2": {"mode":"auto","level":0,"rampup_curve":[1,2,3,4,5],"rampdown_curve":[1,2,3,4,5]},
            "fan3": {"mode":"auto","level":0,"rampup_curve":[1,2,3,4,5],"rampdown_curve":[1,2,3,4,5]},
            "close_to_tray": true,
            "start_with_windows": false,
            "language": "zh",
            "temp_alert_enabled": true,
            "temp_alert_celsius": 90
        }"#;
        assert!(PortableConfig::parse_json(json).is_err());
    }

    #[test]
    fn portable_import_rejects_bad_mode() {
        let mut portable = ServerConfig::default().to_portable();
        portable.fan1.mode = "turbo".into();
        assert!(portable.validate().is_err());
    }

    #[test]
    fn portable_import_rejects_bad_temperature_source() {
        let mut portable = ServerConfig::default().to_portable();
        portable.temperature_source = "hottest".into();
        assert!(portable.validate().is_err());
    }

    #[test]
    fn portable_import_rejects_bad_smoothing_window() {
        let mut portable = ServerConfig::default().to_portable();
        portable.smoothing_window = 0;
        assert!(portable.validate().is_err());
        portable.smoothing_window = 21;
        assert!(portable.validate().is_err());
    }

    #[test]
    fn clamp_smoothing_window_keeps_one_to_twenty() {
        assert_eq!(clamp_smoothing_window(0), 1);
        assert_eq!(clamp_smoothing_window(8), 8);
        assert_eq!(clamp_smoothing_window(99), 20);
    }

    #[test]
    fn portable_import_accepts_valid_file() {
        let json = serde_json::to_string_pretty(&ServerConfig::default().to_portable()).unwrap();
        let parsed = PortableConfig::parse_json(&json).unwrap();
        assert_eq!(parsed.language, "en");
        assert_eq!(parsed.fan1.mode, "auto");
        assert_eq!(parsed.fan1.name, None);
    }

    #[test]
    fn old_fan_config_without_name_parses() {
        let parsed: FanConfig = serde_json::from_str(
            r#"{"mode":"auto","level":0,"rampup_curve":[1,2,3,4,5],"rampdown_curve":[1,2,3,4,5]}"#,
        )
        .unwrap();
        assert_eq!(parsed.name, None);
    }

    #[test]
    fn processor_custom_name_roundtrips() {
        let mut config = ServerConfig::default();
        config.processor_name = Some("  MiniPC  ".into());
        config.processor_name = config.processor_name.as_deref().and_then(sanitize_name);
        assert_eq!(config.processor_custom_name().as_deref(), Some("MiniPC"));
        let json = serde_json::to_string(&config.to_portable()).unwrap();
        assert!(json.contains("MiniPC"));
        let parsed = PortableConfig::parse_json(&json).unwrap();
        assert_eq!(parsed.processor_name.as_deref(), Some("MiniPC"));
    }

    #[test]
    fn fan_custom_name_roundtrips() {
        let mut config = ServerConfig::default();
        config.fan1.as_mut().unwrap().name = Some("  Exhaust  ".into());
        sanitize_fan_names(&mut config);
        assert_eq!(config.fan_custom_name(1).as_deref(), Some("Exhaust"));
        let json = serde_json::to_string(&config.to_portable()).unwrap();
        assert!(json.contains("Exhaust"));
        let parsed = PortableConfig::parse_json(&json).unwrap();
        assert_eq!(parsed.fan1.name.as_deref(), Some("Exhaust"));
    }
}
