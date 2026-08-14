use std::sync::Mutex;

use crate::ec_io::{is_acpi_ec_port, EcIoBackend, EC_COMMAND_PORT, EC_DATA_PORT};
use crate::hardware::HardwareIdentity;
use crate::platform::AccessEcGuard;

const EC_COMMAND_READ: u8 = 0x80;
const EC_COMMAND_WRITE: u8 = 0x81;
const RW_TIMEOUT: u32 = 500;
const MAX_RETRIES: u32 = 5;

const EC_STATUS_OUTPUT_BUFFER_FULL: u8 = 0x01;
const EC_STATUS_INPUT_BUFFER_FULL: u8 = 0x02;

const EC_REG_FIRMWARE_MAJOR: u8 = 0x00;
const EC_REG_FIRMWARE_MINOR: u8 = 0x01;
pub const EC_REG_APU_POWER_MODE: u8 = 0x31;
const EC_REG_APU_TEMPERATURE: u8 = 0x70;

const EC_REG_FAN1_SPEED_HIGH: u8 = 0x35;
const EC_REG_FAN1_SPEED_LOW: u8 = 0x36;
const EC_REG_FAN1_MODE: u8 = 0x21;

const EC_REG_FAN2_SPEED_HIGH: u8 = 0x37;
const EC_REG_FAN2_SPEED_LOW: u8 = 0x38;
const EC_REG_FAN2_MODE: u8 = 0x23;

const EC_REG_FAN3_SPEED_HIGH: u8 = 0x28;
const EC_REG_FAN3_SPEED_LOW: u8 = 0x29;
const EC_REG_FAN3_MODE: u8 = 0x25;

pub const MIN_FIRMWARE_MAJOR: u8 = 1;
pub const MIN_FIRMWARE_MINOR: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    Balanced,
    Performance,
    Quiet,
}

impl PowerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerMode::Balanced => "balanced",
            PowerMode::Performance => "performance",
            PowerMode::Quiet => "quiet",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "balanced" => Some(PowerMode::Balanced),
            "performance" => Some(PowerMode::Performance),
            "quiet" => Some(PowerMode::Quiet),
            _ => None,
        }
    }

    pub fn to_ec_value(self) -> u8 {
        match self {
            PowerMode::Balanced => 0x00,
            PowerMode::Performance => 0x01,
            PowerMode::Quiet => 0x02,
        }
    }

    pub fn from_ec_value(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(PowerMode::Balanced),
            0x01 => Some(PowerMode::Performance),
            0x02 => Some(PowerMode::Quiet),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum EcOperation {
    GetFirmwareVersion,
    GetApuPowerMode,
    SetApuPowerMode(String),
    GetApuTemperature,
    GetFanRpm(u8),
    GetFanMode(u8),
    SetFanMode(u8, String),
    GetFanLevel(u8),
    SetFanLevel(u8, u8),
    GetFanRampupCurve(u8),
    SetFanRampupCurve(u8, [u8; 5]),
    GetFanRampdownCurve(u8),
    SetFanRampdownCurve(u8, [u8; 5]),
}

#[derive(Debug, Clone)]
pub enum EcResult {
    FirmwareVersion { major: u8, minor: u8 },
    ApuPowerMode(String),
    ApuTemperature(u8),
    FanRpm(u16),
    FanMode(String),
    FanLevel(u8),
    FanRampupCurve([u8; 5]),
    FanRampdownCurve([u8; 5]),
}

#[derive(Debug, Clone, Copy)]
pub struct FanCurveData {
    pub rampup_curve: [u8; 5],
    pub rampdown_curve: [u8; 5],
    pub mode: FanMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanMode {
    Auto,
    Fixed,
    Curve,
}

impl FanMode {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            FanMode::Auto => "auto",
            FanMode::Fixed => "fixed",
            FanMode::Curve => "curve",
        }
    }

    pub fn from_name(s: &str) -> Option<FanMode> {
        match s {
            "auto" => Some(FanMode::Auto),
            "fixed" => Some(FanMode::Fixed),
            "curve" => Some(FanMode::Curve),
            _ => None,
        }
    }
}

impl Default for FanCurveData {
    fn default() -> Self {
        FanCurveData {
            rampup_curve: [60, 70, 83, 95, 97],
            rampdown_curve: [40, 50, 80, 94, 96],
            mode: FanMode::Auto,
        }
    }
}

pub struct EcController<B: EcIoBackend> {
    backend: B,
    transaction: Mutex<()>,
    fan_curves: Mutex<[FanCurveData; 3]>,
    writes_allowed: bool,
    hardware: HardwareIdentity,
}

impl<B: EcIoBackend> EcController<B> {
    pub fn new_unchecked(backend: B) -> Self {
        let mut curves = [FanCurveData::default(); 3];
        curves[2].rampup_curve = [20, 60, 83, 95, 97];
        curves[2].rampdown_curve = [0, 50, 80, 94, 96];

        EcController {
            backend,
            transaction: Mutex::new(()),
            fan_curves: Mutex::new(curves),
            writes_allowed: true,
            hardware: HardwareIdentity::default(),
        }
    }

    pub fn initialize(backend: B, hardware: HardwareIdentity) -> Result<Self, String> {
        let mut controller = Self::new_unchecked(backend);
        controller.hardware = hardware;

        let (major, minor) = controller.read_firmware_version()?;
        if !firmware_supported(major, minor) {
            return Err(format!(
                "Unsupported EC firmware {major}.{:02}. This software requires EC firmware {MIN_FIRMWARE_MAJOR}.{MIN_FIRMWARE_MINOR:02} or higher.",
                minor
            ));
        }

        if !controller.hardware.is_supported_axb35() {
            controller.writes_allowed = false;
        }

        Ok(controller)
    }

    pub fn writes_allowed(&self) -> bool {
        self.writes_allowed
    }

    pub fn hardware(&self) -> &HardwareIdentity {
        &self.hardware
    }

    pub fn unsupported_write_error(&self) -> String {
        format!(
            "Unsupported hardware '{}'. EC writes are disabled to avoid damaging unrelated machines.",
            self.hardware.summary()
        )
    }

    pub fn execute_operation(&self, operation: EcOperation) -> Result<EcResult, String> {
        match operation {
            EcOperation::GetFirmwareVersion => {
                let (major, minor) = self.read_firmware_version()?;
                Ok(EcResult::FirmwareVersion { major, minor })
            }
            EcOperation::GetApuPowerMode => {
                let mode_val = self.read_byte(EC_REG_APU_POWER_MODE)?;
                let mode = PowerMode::from_ec_value(mode_val)
                    .ok_or_else(|| format!("Unknown power mode: 0x{mode_val:02X}"))?;
                Ok(EcResult::ApuPowerMode(mode.as_str().to_string()))
            }
            EcOperation::SetApuPowerMode(mode) => {
                self.ensure_writes_allowed()?;
                let parsed = PowerMode::from_name(&mode)
                    .ok_or_else(|| format!("Invalid power mode: {mode}"))?;
                self.write_byte(EC_REG_APU_POWER_MODE, parsed.to_ec_value())?;
                Ok(EcResult::ApuPowerMode(parsed.as_str().to_string()))
            }
            EcOperation::GetApuTemperature => {
                let temp = self.read_byte(EC_REG_APU_TEMPERATURE)?;
                Ok(EcResult::ApuTemperature(temp))
            }
            EcOperation::GetFanRpm(fan_id) => {
                let (high_reg, low_reg) = self.get_fan_speed_registers(fan_id)?;
                let high = self.read_byte(high_reg)?;
                let low = self.read_byte(low_reg)?;
                let mut rpm = ((high as u16) << 8) | (low as u16);

                if fan_id == 3 && rpm == 8000 {
                    rpm = 0;
                }

                Ok(EcResult::FanRpm(rpm))
            }
            EcOperation::GetFanMode(fan_id) => {
                let mode_reg = self.get_fan_mode_register(fan_id)?;
                let mode_val = self.read_byte(mode_reg)?;

                let curves = self.fan_curves.lock().unwrap();
                let fan_idx = (fan_id - 1) as usize;

                let mode = match mode_val {
                    0x10 | 0x20 | 0x30 => "auto",
                    0x11 | 0x21 | 0x31 => {
                        if curves[fan_idx].mode == FanMode::Curve {
                            "curve"
                        } else {
                            "fixed"
                        }
                    }
                    _ => return Err(format!("Unknown fan mode: 0x{mode_val:02X}")),
                };

                Ok(EcResult::FanMode(mode.to_string()))
            }
            EcOperation::SetFanMode(fan_id, mode) => {
                self.ensure_writes_allowed()?;
                let mode_reg = self.get_fan_mode_register(fan_id)?;
                let base_val = match fan_id {
                    1 => 0x10,
                    2 => 0x20,
                    3 => 0x30,
                    _ => return Err(format!("Invalid fan ID: {fan_id}")),
                };

                let fan_mode =
                    FanMode::from_name(&mode).ok_or_else(|| format!("Invalid fan mode: {mode}"))?;

                let mode_val = match fan_mode {
                    FanMode::Auto => base_val,
                    FanMode::Fixed | FanMode::Curve => base_val + 1,
                };

                {
                    let mut curves = self.fan_curves.lock().unwrap();
                    let fan_idx = (fan_id - 1) as usize;
                    curves[fan_idx].mode = fan_mode;
                }

                self.write_byte(mode_reg, mode_val)?;

                if fan_mode == FanMode::Curve {
                    if let Ok(temp) = self.read_byte(EC_REG_APU_TEMPERATURE) {
                        let curves = self.fan_curves.lock().unwrap();
                        let fan_idx = (fan_id - 1) as usize;
                        let mut initial_level = 0;

                        for i in (1..=5).rev() {
                            if temp >= curves[fan_idx].rampup_curve[i - 1] {
                                initial_level = i as u8;
                                break;
                            }
                        }

                        drop(curves);
                        self.write_fan_level(fan_id, initial_level)?;
                    }
                }

                Ok(EcResult::FanMode(mode))
            }
            EcOperation::GetFanLevel(fan_id) => {
                let mode_reg = self.get_fan_mode_register(fan_id)?;
                let level_val = self.read_byte(mode_reg + 1)?;
                Ok(EcResult::FanLevel(decode_fan_level(level_val)))
            }
            EcOperation::SetFanLevel(fan_id, level) => {
                self.ensure_writes_allowed()?;
                if level > 5 {
                    return Err("Fan level must be 0-5".to_string());
                }
                self.write_fan_level(fan_id, level)?;
                Ok(EcResult::FanLevel(level))
            }
            EcOperation::GetFanRampupCurve(fan_id) => {
                if !(1..=3).contains(&fan_id) {
                    return Err(format!("Invalid fan ID: {fan_id}"));
                }
                let curves = self.fan_curves.lock().unwrap();
                let fan_idx = (fan_id - 1) as usize;
                Ok(EcResult::FanRampupCurve(curves[fan_idx].rampup_curve))
            }
            EcOperation::SetFanRampupCurve(fan_id, curve) => {
                if !(1..=3).contains(&fan_id) {
                    return Err(format!("Invalid fan ID: {fan_id}"));
                }
                for &temp in &curve {
                    if temp > 100 {
                        return Err("Temperature values must be 0-100°C".to_string());
                    }
                }
                let mut curves = self.fan_curves.lock().unwrap();
                let fan_idx = (fan_id - 1) as usize;
                curves[fan_idx].rampup_curve = curve;
                Ok(EcResult::FanRampupCurve(curve))
            }
            EcOperation::GetFanRampdownCurve(fan_id) => {
                if !(1..=3).contains(&fan_id) {
                    return Err(format!("Invalid fan ID: {fan_id}"));
                }
                let curves = self.fan_curves.lock().unwrap();
                let fan_idx = (fan_id - 1) as usize;
                Ok(EcResult::FanRampdownCurve(curves[fan_idx].rampdown_curve))
            }
            EcOperation::SetFanRampdownCurve(fan_id, curve) => {
                if !(1..=3).contains(&fan_id) {
                    return Err(format!("Invalid fan ID: {fan_id}"));
                }
                for &temp in &curve {
                    if temp > 100 {
                        return Err("Temperature values must be 0-100°C".to_string());
                    }
                }
                let mut curves = self.fan_curves.lock().unwrap();
                let fan_idx = (fan_id - 1) as usize;
                curves[fan_idx].rampdown_curve = curve;
                Ok(EcResult::FanRampdownCurve(curve))
            }
        }
    }

    fn ensure_writes_allowed(&self) -> Result<(), String> {
        if self.writes_allowed {
            Ok(())
        } else {
            Err(self.unsupported_write_error())
        }
    }

    fn get_fan_speed_registers(&self, fan_id: u8) -> Result<(u8, u8), String> {
        match fan_id {
            1 => Ok((EC_REG_FAN1_SPEED_HIGH, EC_REG_FAN1_SPEED_LOW)),
            2 => Ok((EC_REG_FAN2_SPEED_HIGH, EC_REG_FAN2_SPEED_LOW)),
            3 => Ok((EC_REG_FAN3_SPEED_HIGH, EC_REG_FAN3_SPEED_LOW)),
            _ => Err(format!("Invalid fan ID: {fan_id}")),
        }
    }

    fn get_fan_mode_register(&self, fan_id: u8) -> Result<u8, String> {
        match fan_id {
            1 => Ok(EC_REG_FAN1_MODE),
            2 => Ok(EC_REG_FAN2_MODE),
            3 => Ok(EC_REG_FAN3_MODE),
            _ => Err(format!("Invalid fan ID: {fan_id}")),
        }
    }

    fn write_fan_level(&self, fan_id: u8, level: u8) -> Result<(), String> {
        if level > 5 {
            return Err("Fan level must be 0-5".to_string());
        }

        let mode_reg = self.get_fan_mode_register(fan_id)?;
        let base_val = match fan_id {
            1 => 0x10,
            2 => 0x20,
            3 => 0x30,
            _ => return Err(format!("Invalid fan ID: {fan_id}")),
        };

        let level_val = base_val
            + match level {
                0 => 0x7,
                1 => 0x2,
                2 => 0x3,
                3 => 0x4,
                4 => 0x5,
                5 => 0x6,
                _ => 0x7,
            };

        self.write_byte(mode_reg + 1, level_val)
    }

    fn read_fan_level(&self, fan_id: u8) -> Result<u8, String> {
        let mode_reg = self.get_fan_mode_register(fan_id)?;
        let level_val = self.read_byte(mode_reg + 1)?;
        Ok(decode_fan_level(level_val))
    }

    pub fn update_curve_fans(&self) -> Result<Vec<String>, String> {
        if !self.writes_allowed {
            return Ok(Vec::new());
        }

        let mut log_messages = Vec::new();
        let temp = self.read_byte(EC_REG_APU_TEMPERATURE)?;
        let curves = self.fan_curves.lock().unwrap();

        for fan_id in 1..=3 {
            let fan_idx = (fan_id - 1) as usize;

            if curves[fan_idx].mode == FanMode::Curve {
                let current_level = self.read_fan_level(fan_id)?;
                let mut new_level = current_level;

                if current_level < 5 && temp >= curves[fan_idx].rampup_curve[current_level as usize]
                {
                    new_level = current_level + 1;
                    log_messages.push(format!(
                        "Fan{fan_id} ramping up to level {new_level} (temp: {temp}°C, threshold: {}°C)",
                        curves[fan_idx].rampup_curve[current_level as usize]
                    ));
                } else if current_level > 0
                    && temp <= curves[fan_idx].rampdown_curve[(current_level - 1) as usize]
                {
                    new_level = current_level - 1;
                    log_messages.push(format!(
                        "Fan{fan_id} ramping down to level {new_level} (temp: {temp}°C, threshold: {}°C)",
                        curves[fan_idx].rampdown_curve[(current_level - 1) as usize]
                    ));
                }

                if new_level != current_level {
                    drop(curves);
                    self.write_fan_level(fan_id, new_level)?;
                    return Ok(log_messages);
                }
            }
        }

        Ok(log_messages)
    }

    pub fn has_curve_fans(&self) -> bool {
        let curves = self.fan_curves.lock().unwrap();
        curves.iter().any(|curve| curve.mode == FanMode::Curve)
    }

    fn read_firmware_version(&self) -> Result<(u8, u8), String> {
        let major = self.read_byte(EC_REG_FIRMWARE_MAJOR)?;
        let minor = self.read_byte(EC_REG_FIRMWARE_MINOR)?;
        if (major == 0 && minor == 0) || (major == 0xFF && minor == 0xFF) {
            return Err("Invalid firmware version detected".to_string());
        }
        Ok((major, minor))
    }

    fn with_transaction<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _local = self
            .transaction
            .lock()
            .map_err(|_| "EC transaction lock poisoned".to_string())?;
        let _access_ec = AccessEcGuard::acquire()?;
        f()
    }

    fn read_io_port(&self, port: u16) -> Result<u8, String> {
        if !is_acpi_ec_port(port) {
            return Err(format!("Refusing EC I/O on non-ACPI port 0x{port:02X}"));
        }
        self.backend.read_port_u8(port)
    }

    fn write_io_port(&self, port: u16, value: u8) -> Result<(), String> {
        if !is_acpi_ec_port(port) {
            return Err(format!("Refusing EC I/O on non-ACPI port 0x{port:02X}"));
        }
        self.backend.write_port_u8(port, value)
    }

    fn wait_for_ec_status(&self, status: u8, is_set: bool) -> Result<(), String> {
        for _ in 0..RW_TIMEOUT {
            let mut value = self.read_io_port(EC_COMMAND_PORT)?;
            if is_set {
                value = !value;
            }
            if (status & value) == 0 {
                return Ok(());
            }
        }
        if is_set {
            Err("EC timeout waiting for output buffer to fill".to_string())
        } else {
            Err("EC timeout waiting for input buffer to clear".to_string())
        }
    }

    fn wait_write(&self) -> Result<(), String> {
        self.wait_for_ec_status(EC_STATUS_INPUT_BUFFER_FULL, false)
    }

    fn wait_read(&self) -> Result<(), String> {
        self.wait_for_ec_status(EC_STATUS_OUTPUT_BUFFER_FULL, true)
    }

    fn try_read_byte(&self, register: u8) -> Result<u8, String> {
        self.wait_write()?;
        self.write_io_port(EC_COMMAND_PORT, EC_COMMAND_READ)?;
        self.wait_write()?;
        self.write_io_port(EC_DATA_PORT, register)?;
        self.wait_write()?;
        self.wait_read()?;
        self.read_io_port(EC_DATA_PORT)
    }

    fn try_write_byte(&self, register: u8, value: u8) -> Result<(), String> {
        self.wait_write()?;
        self.write_io_port(EC_COMMAND_PORT, EC_COMMAND_WRITE)?;
        self.wait_write()?;
        self.write_io_port(EC_DATA_PORT, register)?;
        self.wait_write()?;
        self.write_io_port(EC_DATA_PORT, value)?;
        Ok(())
    }

    fn read_byte(&self, register: u8) -> Result<u8, String> {
        self.with_transaction(|| {
            let mut last_error = "Failed to read byte after retries".to_string();
            for attempt in 0..MAX_RETRIES {
                match self.try_read_byte(register) {
                    Ok(value) => return Ok(value),
                    Err(error) => {
                        last_error = error;
                        if attempt + 1 < MAX_RETRIES {
                            eprintln!("WARN  EC read retry {}/{MAX_RETRIES}", attempt + 1);
                        }
                    }
                }
            }
            Err(last_error)
        })
    }

    fn write_byte(&self, register: u8, value: u8) -> Result<(), String> {
        self.with_transaction(|| {
            let mut last_error = "Failed to write byte after retries".to_string();
            for attempt in 0..MAX_RETRIES {
                match self.try_write_byte(register, value) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        last_error = error;
                        if attempt + 1 < MAX_RETRIES {
                            eprintln!("WARN  EC write retry {}/{MAX_RETRIES}", attempt + 1);
                        }
                    }
                }
            }
            Err(last_error)
        })
    }
}

fn decode_fan_level(level_val: u8) -> u8 {
    match level_val & 0xF {
        0x7 => 0,
        0x2 => 1,
        0x3 => 2,
        0x4 => 3,
        0x5 => 4,
        0x6 => 5,
        _ => 0,
    }
}

pub fn firmware_supported(major: u8, minor: u8) -> bool {
    major > MIN_FIRMWARE_MAJOR || (major == MIN_FIRMWARE_MAJOR && minor >= MIN_FIRMWARE_MINOR)
}

pub fn format_firmware_version(major: u8, minor: u8) -> String {
    if minor < 10 {
        format!("{major}.0{minor}")
    } else {
        format!("{major}.{minor}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ec_io::{MockEcIoBackend, PortOp};
    use std::sync::Arc;
    use std::thread;

    fn write_transactions(ops: &[PortOp]) -> Vec<Vec<PortOp>> {
        let mut groups = Vec::new();
        let mut current = Vec::new();
        for op in ops {
            if let PortOp::Write {
                port: EC_COMMAND_PORT,
                value: EC_COMMAND_WRITE,
            } = op
            {
                if !current.is_empty() {
                    groups.push(current);
                    current = Vec::new();
                }
            }
            if matches!(
                op,
                PortOp::Write {
                    port: EC_COMMAND_PORT,
                    value: EC_COMMAND_WRITE | EC_COMMAND_READ
                }
            ) || !current.is_empty()
            {
                current.push(op.clone());
            }
        }
        if !current.is_empty() {
            groups.push(current);
        }
        groups
    }

    #[test]
    fn power_mode_encoding_matches_upstream() {
        assert_eq!(PowerMode::Balanced.to_ec_value(), 0x00);
        assert_eq!(PowerMode::Performance.to_ec_value(), 0x01);
        assert_eq!(PowerMode::Quiet.to_ec_value(), 0x02);
        assert_eq!(PowerMode::from_ec_value(0x00), Some(PowerMode::Balanced));
        assert_eq!(PowerMode::from_ec_value(0x01), Some(PowerMode::Performance));
        assert_eq!(PowerMode::from_ec_value(0x02), Some(PowerMode::Quiet));
        assert_eq!(PowerMode::from_name("balanced"), Some(PowerMode::Balanced));
        assert_eq!(PowerMode::from_name("turbo"), None);
    }

    #[test]
    fn set_power_mode_writes_register_0x31() {
        let mock = MockEcIoBackend::with_firmware(1, 4);
        let controller = EcController::new_unchecked(mock.clone());

        controller
            .execute_operation(EcOperation::SetApuPowerMode("performance".into()))
            .unwrap();

        assert_eq!(mock.register(EC_REG_APU_POWER_MODE), 0x01);

        let mode = controller
            .execute_operation(EcOperation::GetApuPowerMode)
            .unwrap();
        assert!(matches!(mode, EcResult::ApuPowerMode(ref value) if value == "performance"));

        let writes: Vec<_> = mock
            .ops()
            .into_iter()
            .filter_map(|op| match op {
                PortOp::Write { port, value } => Some((port, value)),
                _ => None,
            })
            .collect();
        assert!(writes.contains(&(EC_COMMAND_PORT, EC_COMMAND_WRITE)));
        assert!(writes.contains(&(EC_DATA_PORT, EC_REG_APU_POWER_MODE)));
        assert!(writes.contains(&(EC_DATA_PORT, 0x01)));
    }

    #[test]
    fn quiet_and_balanced_use_upstream_values() {
        let mock = MockEcIoBackend::with_firmware(1, 4);
        let controller = EcController::new_unchecked(mock.clone());

        controller
            .execute_operation(EcOperation::SetApuPowerMode("quiet".into()))
            .unwrap();
        assert_eq!(mock.register(EC_REG_APU_POWER_MODE), 0x02);

        controller
            .execute_operation(EcOperation::SetApuPowerMode("balanced".into()))
            .unwrap();
        assert_eq!(mock.register(EC_REG_APU_POWER_MODE), 0x00);
    }

    #[test]
    fn invalid_power_mode_never_reaches_hardware() {
        let mock = MockEcIoBackend::with_firmware(1, 4);
        let controller = EcController::new_unchecked(mock.clone());
        let result = controller.execute_operation(EcOperation::SetApuPowerMode("turbo".into()));
        assert!(result.unwrap_err().contains("Invalid power mode"));
        assert!(mock.ops().is_empty());
        assert_eq!(mock.register(EC_REG_APU_POWER_MODE), 0);
    }

    #[test]
    fn ibf_stuck_times_out() {
        let mock = MockEcIoBackend::with_ibf_stuck();
        let controller = EcController::new_unchecked(mock);
        let error = controller
            .execute_operation(EcOperation::GetApuPowerMode)
            .unwrap_err();
        assert!(
            error.contains("EC timeout waiting for input buffer to clear"),
            "{error}"
        );
    }

    #[test]
    fn write_transaction_uses_expected_order() {
        let mock = MockEcIoBackend::with_firmware(1, 4);
        let controller = EcController::new_unchecked(mock.clone());
        controller
            .execute_operation(EcOperation::SetApuPowerMode("quiet".into()))
            .unwrap();

        let writes: Vec<_> = mock
            .ops()
            .into_iter()
            .filter_map(|op| match op {
                PortOp::Write { port, value } => Some((port, value)),
                _ => None,
            })
            .collect();

        assert_eq!(
            writes,
            vec![
                (EC_COMMAND_PORT, EC_COMMAND_WRITE),
                (EC_DATA_PORT, EC_REG_APU_POWER_MODE),
                (EC_DATA_PORT, PowerMode::Quiet.to_ec_value()),
            ]
        );
    }

    #[test]
    fn concurrent_writes_do_not_interleave_transactions() {
        let mock = MockEcIoBackend::with_firmware(1, 4);
        let controller = Arc::new(EcController::new_unchecked(mock.clone()));
        let modes = ["quiet", "balanced", "performance"];
        let mut handles = Vec::new();
        for mode in modes {
            let controller = Arc::clone(&controller);
            handles.push(thread::spawn(move || {
                controller
                    .execute_operation(EcOperation::SetApuPowerMode(mode.to_string()))
                    .unwrap();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let groups = write_transactions(&mock.ops());
        assert_eq!(groups.len(), 3);
        for group in groups {
            let writes: Vec<_> = group
                .into_iter()
                .filter_map(|op| match op {
                    PortOp::Write { port, value } => Some((port, value)),
                    _ => None,
                })
                .collect();
            assert_eq!(writes.len(), 3);
            assert_eq!(writes[0], (EC_COMMAND_PORT, EC_COMMAND_WRITE));
            assert_eq!(writes[1], (EC_DATA_PORT, EC_REG_APU_POWER_MODE));
            assert!(PowerMode::from_ec_value(writes[2].1).is_some());
        }
    }

    #[test]
    fn firmware_1_04_is_the_minimum() {
        assert!(!firmware_supported(1, 3));
        assert!(firmware_supported(1, 4));
        assert!(firmware_supported(2, 0));
        assert_eq!(format_firmware_version(1, 4), "1.04");
    }

    #[test]
    fn unknown_hardware_blocks_writes() {
        let mock = MockEcIoBackend::with_firmware(1, 4);
        let hardware = HardwareIdentity {
            system_manufacturer: "Dell Inc.".into(),
            system_product: "OptiPlex".into(),
            ..HardwareIdentity::default()
        };
        let controller = EcController::initialize(mock.clone(), hardware).unwrap();
        assert!(!controller.writes_allowed());
        let error = controller
            .execute_operation(EcOperation::SetApuPowerMode("quiet".into()))
            .unwrap_err();
        assert!(error.contains("Unsupported hardware"));
        assert_eq!(mock.register(EC_REG_APU_POWER_MODE), 0);
    }
}
