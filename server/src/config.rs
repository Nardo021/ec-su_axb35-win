use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::alert::{clamp_threshold, TEMP_ALERT_DEFAULT};
use crate::i18n::Language;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FanConfig {
    pub mode: String,
    pub level: u8,
    pub rampup_curve: [u8; 5],
    pub rampdown_curve: [u8; 5],
}

impl Default for FanConfig {
    fn default() -> Self {
        FanConfig {
            mode: "auto".to_string(),
            level: 0,
            rampup_curve: [60, 70, 83, 95, 97],
            rampdown_curve: [40, 50, 80, 94, 96],
        }
    }
}

fn default_close_to_tray() -> bool {
    true
}

fn default_language() -> String {
    "zh".to_string()
}

fn default_temp_alert_enabled() -> bool {
    true
}

fn default_temp_alert_celsius() -> u8 {
    TEMP_ALERT_DEFAULT
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        let fan3_config = FanConfig {
            rampup_curve: [20, 60, 83, 95, 97],
            rampdown_curve: [0, 50, 80, 94, 96],
            ..FanConfig::default()
        };

        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8395,
            log_path: format!("{}\\server.log", program_data_dir()),
            apu_power_mode: None,
            fan1: Some(FanConfig::default()),
            fan2: Some(FanConfig::default()),
            fan3: Some(fan3_config),
            close_to_tray: true,
            start_with_windows: false,
            language: default_language(),
            temp_alert_enabled: default_temp_alert_enabled(),
            temp_alert_celsius: default_temp_alert_celsius(),
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
        }
    }

    pub fn apply_portable(&mut self, portable: PortableConfig) {
        self.apu_power_mode = portable.apu_power_mode;
        self.fan1 = Some(portable.fan1);
        self.fan2 = Some(portable.fan2);
        self.fan3 = Some(portable.fan3);
        self.close_to_tray = portable.close_to_tray;
        self.start_with_windows = portable.start_with_windows;
        self.language = portable.language;
        self.temp_alert_enabled = portable.temp_alert_enabled;
        self.temp_alert_celsius = clamp_threshold(portable.temp_alert_celsius);
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(parsed.language, "zh");
        assert_eq!(parsed.language(), Language::Zh);
        assert!(parsed.temp_alert_enabled);
        assert_eq!(parsed.temp_alert_celsius, 90);
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
    fn portable_import_accepts_valid_file() {
        let json = serde_json::to_string_pretty(&ServerConfig::default().to_portable()).unwrap();
        let parsed = PortableConfig::parse_json(&json).unwrap();
        assert_eq!(parsed.language, "zh");
        assert_eq!(parsed.fan1.mode, "auto");
    }
}
