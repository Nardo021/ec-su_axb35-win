use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::{FanConfig, ServerConfig};
use crate::ec::{format_firmware_version, EcController, EcOperation, EcResult};
use crate::hardware::HardwareIdentity;
use crate::logger::Logger;
use crate::pawnio::{lpcacpiec_loaded, pawnio_connected, PawnIoBackend, PAWNIO_MISSING_MESSAGE};
use crate::platform::{is_admin, pawnio_install_version, secure_boot_status, InstanceGuard};
use crate::thermal::{TemperatureSource, ThermalSnapshot};

pub type LiveController = Arc<EcController<PawnIoBackend>>;

#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub pawnio: String,
    pub lpcacpiec: String,
    pub secure_boot: String,
    pub hardware: String,
    pub hardware_supported: bool,
}

#[derive(Debug, Clone)]
pub struct FanSnapshot {
    pub mode: String,
    pub level: u8,
    pub rpm: u16,
    pub rampup_curve: [u8; 5],
    pub rampdown_curve: [u8; 5],
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub power_mode: String,
    pub temperature: u8,
    pub temperature_source: TemperatureSource,
    pub gpu_temp: Option<u8>,
    pub cpu_temp: Option<u8>,
    pub soc_temp: Option<u8>,
    pub hotspot_temp: Option<u8>,
    pub ec_raw_temp: u8,
    pub fans: [FanSnapshot; 3],
}

pub struct StartOptions {
    pub service_mode: bool,
    pub exclusive: bool,
    pub restore: bool,
    pub quiet: bool,
    pub monitor_curves: bool,
}

pub struct AppSession {
    pub controller: LiveController,
    pub config: Arc<Mutex<ServerConfig>>,
    pub logger: Arc<Mutex<Logger>>,
    pub runtime: Arc<RuntimeStatus>,
    stop: Arc<AtomicBool>,
    _instance: Option<InstanceGuard>,
}

impl AppSession {
    pub fn start(options: StartOptions) -> Result<Self, String> {
        if !is_admin() {
            return Err(
                "This application must be run as Administrator to access the EC through PawnIO."
                    .to_string(),
            );
        }

        let instance = if options.exclusive {
            Some(InstanceGuard::acquire()?)
        } else {
            None
        };

        let config = Arc::new(Mutex::new(ServerConfig::load()?));
        let logger = if options.quiet {
            Arc::new(Mutex::new(Logger::silent()))
        } else {
            let config_guard = config.lock().unwrap();
            Arc::new(Mutex::new(Logger::new(
                &config_guard.log_path,
                options.service_mode,
            )?))
        };

        {
            let mut log = logger.lock().unwrap();
            log.info(&format!("{} starting", crate::i18n::APP_NAME));
            log.info("Single-process GUI mode");
            if let Some(version) = pawnio_install_version() {
                log.info(&format!("PawnIO installer version {version}"));
            }
        }

        if !crate::pawnio::probe_device_present() {
            logger.lock().unwrap().error("PawnIO unavailable");
            return Err(PAWNIO_MISSING_MESSAGE.to_string());
        }

        let backend = PawnIoBackend::connect().map_err(|error| {
            logger.lock().unwrap().error("PawnIO unavailable");
            if error.contains("PawnIO is required") {
                error
            } else {
                format!("{PAWNIO_MISSING_MESSAGE}\n\n{error}")
            }
        })?;

        {
            let mut log = logger.lock().unwrap();
            log.info("PawnIO connected");
            log.info("LpcACPIEC module loaded");
            if let Ok(version) = backend.driver_version() {
                log.info(&format!(
                    "PawnIO driver version {}.{}.{}",
                    (version >> 16) & 0xFF,
                    (version >> 8) & 0xFF,
                    version & 0xFF
                ));
            }
        }

        let hardware = HardwareIdentity::detect();
        {
            let mut log = logger.lock().unwrap();
            log.info(&format!("Detected hardware: {}", hardware.summary()));
            if hardware.is_supported_axb35() {
                log.info("EVO-X2 EC platform identified");
            } else {
                log.warn(&format!(
                    "Hardware '{}' is not a known Sixunited AXB35 / GMKtec EVO-X2 board. EC writes will be refused.",
                    hardware.summary()
                ));
            }
            log.info(&format!("Secure Boot: {}", secure_boot_status()));
        }

        let controller = Arc::new(EcController::initialize(backend, hardware)?);
        controller.set_preferred_temperature(config.lock().unwrap().temperature_source());
        {
            let mut log = logger.lock().unwrap();
            log.info("EC controller initialized successfully");
            if pawnio_connected() && lpcacpiec_loaded() {
                log.info("EVO-X2 EC detected");
            }
            if let Ok(ec_temp) = controller.read_ec_apu_temperature() {
                log.info(&crate::thermal::describe_source(
                    ec_temp,
                    controller.preferred_temperature(),
                ));
            }
        }

        let runtime = Arc::new(RuntimeStatus {
            pawnio: if pawnio_connected() {
                "connected".into()
            } else {
                "unavailable".into()
            },
            lpcacpiec: if lpcacpiec_loaded() {
                "loaded".into()
            } else {
                "not loaded".into()
            },
            secure_boot: secure_boot_status(),
            hardware: controller.hardware().summary(),
            hardware_supported: controller.hardware().is_supported_axb35(),
        });

        let session = Self {
            controller,
            config,
            logger,
            runtime,
            stop: Arc::new(AtomicBool::new(false)),
            _instance: instance,
        };

        if options.restore {
            session.restore_saved_state();
        }
        if options.monitor_curves {
            session.spawn_curve_monitor();
        }
        Ok(session)
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.logger
            .lock()
            .unwrap()
            .info(&format!("{} shutting down", crate::i18n::APP_NAME));
    }

    pub fn firmware_version(&self) -> Result<String, String> {
        match self
            .controller
            .execute_operation(EcOperation::GetFirmwareVersion)?
        {
            EcResult::FirmwareVersion { major, minor } => Ok(format_firmware_version(major, minor)),
            _ => Err("Unexpected firmware response".to_string()),
        }
    }

    pub fn metrics(&self) -> Result<MetricsSnapshot, String> {
        let thermal = self.thermal_reading()?;
        Ok(MetricsSnapshot {
            power_mode: self.power_mode()?,
            temperature: thermal.control,
            temperature_source: thermal.control_source,
            gpu_temp: thermal.gpu,
            cpu_temp: thermal.cpu,
            soc_temp: thermal.soc,
            hotspot_temp: thermal.hotspot,
            ec_raw_temp: thermal.ec_raw,
            fans: [
                self.fan_snapshot(1)?,
                self.fan_snapshot(2)?,
                self.fan_snapshot(3)?,
            ],
        })
    }

    pub fn power_mode(&self) -> Result<String, String> {
        match self
            .controller
            .execute_operation(EcOperation::GetApuPowerMode)?
        {
            EcResult::ApuPowerMode(mode) => Ok(mode),
            _ => Err("Unexpected power mode response".to_string()),
        }
    }

    pub fn set_power_mode(&self, mode: &str) -> Result<String, String> {
        let previous = self.power_mode().ok();
        let result = self
            .controller
            .execute_operation(EcOperation::SetApuPowerMode(mode.to_string()))?;
        let EcResult::ApuPowerMode(applied) = result else {
            return Err("Unexpected power mode response".to_string());
        };
        if let Some(previous) = previous {
            self.logger
                .lock()
                .unwrap()
                .info(&format!("power mode changed: {previous} -> {applied}"));
        }
        {
            let mut config = self.config.lock().unwrap();
            config.apu_power_mode = Some(applied.clone());
            if let Err(error) = config.save() {
                self.logger
                    .lock()
                    .unwrap()
                    .warn(&format!("Failed to save power mode: {error}"));
            }
        }
        Ok(applied)
    }

    fn thermal_reading(&self) -> Result<ThermalSnapshot, String> {
        let ec = self.controller.read_ec_apu_temperature()?;
        Ok(crate::thermal::read_thermal(
            ec,
            self.controller.preferred_temperature(),
        ))
    }

    pub fn fan_snapshot(&self, fan_id: u8) -> Result<FanSnapshot, String> {
        let mode = match self
            .controller
            .execute_operation(EcOperation::GetFanMode(fan_id))?
        {
            EcResult::FanMode(mode) => mode,
            _ => return Err(format!("Failed to get Fan{fan_id} mode")),
        };
        let level = match self
            .controller
            .execute_operation(EcOperation::GetFanLevel(fan_id))?
        {
            EcResult::FanLevel(level) => level,
            _ => return Err(format!("Failed to get Fan{fan_id} level")),
        };
        let rpm = match self
            .controller
            .execute_operation(EcOperation::GetFanRpm(fan_id))?
        {
            EcResult::FanRpm(rpm) => rpm,
            _ => return Err(format!("Failed to get Fan{fan_id} RPM")),
        };
        let rampup_curve = match self
            .controller
            .execute_operation(EcOperation::GetFanRampupCurve(fan_id))?
        {
            EcResult::FanRampupCurve(curve) => curve,
            _ => return Err(format!("Failed to get Fan{fan_id} rampup curve")),
        };
        let rampdown_curve = match self
            .controller
            .execute_operation(EcOperation::GetFanRampdownCurve(fan_id))?
        {
            EcResult::FanRampdownCurve(curve) => curve,
            _ => return Err(format!("Failed to get Fan{fan_id} rampdown curve")),
        };
        Ok(FanSnapshot {
            mode,
            level,
            rpm,
            rampup_curve,
            rampdown_curve,
        })
    }

    pub fn set_fan_mode(&self, fan_id: u8, mode: &str) -> Result<String, String> {
        let result = self
            .controller
            .execute_operation(EcOperation::SetFanMode(fan_id, mode.to_string()))?;
        let EcResult::FanMode(applied) = result else {
            return Err("Unexpected fan mode response".to_string());
        };
        self.update_fan_config(fan_id, |fan| fan.mode = applied.clone());
        Ok(applied)
    }

    pub fn set_fan_level(&self, fan_id: u8, level: u8) -> Result<u8, String> {
        let result = self
            .controller
            .execute_operation(EcOperation::SetFanLevel(fan_id, level))?;
        let EcResult::FanLevel(applied) = result else {
            return Err("Unexpected fan level response".to_string());
        };
        self.update_fan_config(fan_id, |fan| fan.level = applied);
        Ok(applied)
    }

    pub fn set_fan_rampup(&self, fan_id: u8, curve: [u8; 5]) -> Result<[u8; 5], String> {
        let result = self
            .controller
            .execute_operation(EcOperation::SetFanRampupCurve(fan_id, curve))?;
        let EcResult::FanRampupCurve(applied) = result else {
            return Err("Unexpected rampup response".to_string());
        };
        self.update_fan_config(fan_id, |fan| fan.rampup_curve = applied);
        Ok(applied)
    }

    pub fn set_fan_rampdown(&self, fan_id: u8, curve: [u8; 5]) -> Result<[u8; 5], String> {
        let result = self
            .controller
            .execute_operation(EcOperation::SetFanRampdownCurve(fan_id, curve))?;
        let EcResult::FanRampdownCurve(applied) = result else {
            return Err("Unexpected rampdown response".to_string());
        };
        self.update_fan_config(fan_id, |fan| fan.rampdown_curve = applied);
        Ok(applied)
    }

    pub fn apply_fan_edit(
        &self,
        fan_id: u8,
        mode: &str,
        level: u8,
        rampup: Option<[u8; 5]>,
        rampdown: Option<[u8; 5]>,
    ) -> Result<(), String> {
        self.set_fan_mode(fan_id, mode)?;
        if mode == "fixed" {
            self.set_fan_level(fan_id, level)?;
        }
        if mode == "curve" {
            if let Some(curve) = rampup {
                self.set_fan_rampup(fan_id, curve)?;
            }
            if let Some(curve) = rampdown {
                self.set_fan_rampdown(fan_id, curve)?;
            }
        }
        Ok(())
    }

    pub fn restore_saved_state(&self) {
        self.logger
            .lock()
            .unwrap()
            .info("Restoring saved parameters from configuration...");
        if !self.controller.writes_allowed() {
            self.logger
                .lock()
                .unwrap()
                .warn("Skipping configuration restore because EC writes are disabled");
            return;
        }

        let (power_mode, fans) = {
            let config = self.config.lock().unwrap();
            (
                config.apu_power_mode.clone(),
                [
                    config.fan1.clone(),
                    config.fan2.clone(),
                    config.fan3.clone(),
                ],
            )
        };

        if let Some(mode) = power_mode {
            if self.set_power_mode(&mode).is_ok() {
                self.logger
                    .lock()
                    .unwrap()
                    .info(&format!("Restored power mode: {mode}"));
            }
        }

        for (index, fan) in fans.into_iter().enumerate() {
            let fan_id = (index + 1) as u8;
            let Some(fan) = fan else {
                self.logger.lock().unwrap().info(&format!(
                    "Fan{fan_id} configuration not found in config, leaving in original state"
                ));
                continue;
            };
            let _ = self.set_fan_mode(fan_id, &fan.mode);
            if fan.mode != "auto" {
                let _ = self.set_fan_level(fan_id, fan.level);
            }
            let _ = self.set_fan_rampup(fan_id, fan.rampup_curve);
            let _ = self.set_fan_rampdown(fan_id, fan.rampdown_curve);
        }
        self.logger
            .lock()
            .unwrap()
            .info("Parameter restoration completed");
    }

    fn spawn_curve_monitor(&self) {
        let controller = Arc::clone(&self.controller);
        let logger = Arc::clone(&self.logger);
        let stop = Arc::clone(&self.stop);
        thread::spawn(move || {
            let mut active = false;
            while !stop.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(250));
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let has_curve = controller.has_curve_fans();
                if has_curve && !active {
                    logger
                        .lock()
                        .unwrap()
                        .info("Curve monitoring started - fans in curve mode detected");
                    active = true;
                } else if !has_curve && active {
                    logger
                        .lock()
                        .unwrap()
                        .info("Curve monitoring stopped - no fans in curve mode");
                    active = false;
                }
                if !has_curve {
                    continue;
                }
                match controller.update_curve_fans() {
                    Ok(messages) => {
                        let mut log = logger.lock().unwrap();
                        for message in messages {
                            log.info(&message);
                        }
                    }
                    Err(error) => logger
                        .lock()
                        .unwrap()
                        .warn(&format!("Curve monitoring error: {error}")),
                }
            }
        });
    }

    fn update_fan_config(&self, fan_id: u8, update: impl FnOnce(&mut FanConfig)) {
        let mut config = self.config.lock().unwrap();
        let slot = match fan_id {
            1 => &mut config.fan1,
            2 => &mut config.fan2,
            3 => &mut config.fan3,
            _ => return,
        };
        if slot.is_none() {
            *slot = Some(FanConfig::default());
        }
        if let Some(fan) = slot.as_mut() {
            update(fan);
        }
        if let Err(error) = config.save() {
            self.logger
                .lock()
                .unwrap()
                .warn(&format!("Failed to save Fan{fan_id} config: {error}"));
        }
    }

    pub fn set_fan_name(&self, fan_id: u8, name: Option<String>) {
        self.update_fan_config(fan_id, |fan| {
            fan.name = name.as_deref().and_then(crate::fan::sanitize_name);
        });
    }
}
