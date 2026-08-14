#[cfg(test)]
use std::sync::{Arc, Mutex};

pub const EC_COMMAND_PORT: u16 = 0x66;
pub const EC_DATA_PORT: u16 = 0x62;

pub fn is_acpi_ec_port(port: u16) -> bool {
    port == EC_COMMAND_PORT || port == EC_DATA_PORT
}

/// Byte-wide I/O used by the ACPI EC protocol.
///
/// Implementations must not assume they are the only caller; the EC protocol
/// layer serializes complete transactions before invoking these methods.
pub trait EcIoBackend: Send + Sync {
    fn read_port_u8(&self, port: u16) -> Result<u8, String>;
    fn write_port_u8(&self, port: u16, value: u8) -> Result<(), String>;
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortOp {
    Read { port: u16, value: u8 },
    Write { port: u16, value: u8 },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingCommand {
    None,
    Read,
    WriteAddress,
    WriteData(u8),
}

#[cfg(test)]
struct MockEcState {
    ops: Vec<PortOp>,
    registers: [u8; 256],
    ibf_stuck: bool,
    pending: PendingCommand,
    data: u8,
    obf: bool,
}

#[cfg(test)]
impl MockEcState {
    fn new() -> Self {
        let mut registers = [0u8; 256];
        registers[0x00] = 1;
        registers[0x01] = 4;
        Self {
            ops: Vec::new(),
            registers,
            ibf_stuck: false,
            pending: PendingCommand::None,
            data: 0,
            obf: false,
        }
    }

    fn status(&self) -> u8 {
        const OBF: u8 = 0x01;
        const IBF: u8 = 0x02;
        let mut value = 0;
        if self.obf {
            value |= OBF;
        }
        if self.ibf_stuck {
            value |= IBF;
        }
        value
    }

    fn write_command(&mut self, value: u8) {
        self.pending = match value {
            0x80 => PendingCommand::Read,
            0x81 => PendingCommand::WriteAddress,
            _ => PendingCommand::None,
        };
    }

    fn write_data(&mut self, value: u8) {
        match self.pending {
            PendingCommand::Read => {
                self.data = self.registers[value as usize];
                self.obf = true;
                self.pending = PendingCommand::None;
            }
            PendingCommand::WriteAddress => {
                self.pending = PendingCommand::WriteData(value);
            }
            PendingCommand::WriteData(register) => {
                self.registers[register as usize] = value;
                self.pending = PendingCommand::None;
            }
            PendingCommand::None => {}
        }
    }
}

/// In-process ACPI EC that records port traffic for unit tests.
#[cfg(test)]
#[derive(Clone)]
pub struct MockEcIoBackend {
    state: Arc<Mutex<MockEcState>>,
}

#[cfg(test)]
impl MockEcIoBackend {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockEcState::new())),
        }
    }

    pub fn with_firmware(major: u8, minor: u8) -> Self {
        let backend = Self::new();
        backend.set_register(0x00, major);
        backend.set_register(0x01, minor);
        backend
    }

    pub fn with_ibf_stuck() -> Self {
        let backend = Self::new();
        backend.state.lock().unwrap().ibf_stuck = true;
        backend
    }

    pub fn set_register(&self, register: u8, value: u8) {
        self.state.lock().unwrap().registers[register as usize] = value;
    }

    pub fn register(&self, register: u8) -> u8 {
        self.state.lock().unwrap().registers[register as usize]
    }

    pub fn ops(&self) -> Vec<PortOp> {
        self.state.lock().unwrap().ops.clone()
    }
}

#[cfg(test)]
impl Default for MockEcIoBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl EcIoBackend for MockEcIoBackend {
    fn read_port_u8(&self, port: u16) -> Result<u8, String> {
        let mut state = self.state.lock().unwrap();
        let value = match port {
            EC_COMMAND_PORT => state.status(),
            EC_DATA_PORT => {
                let value = state.data;
                state.obf = false;
                value
            }
            _ => return Err(format!("Mock EC refused non-ACPI port 0x{port:02X}")),
        };
        state.ops.push(PortOp::Read { port, value });
        Ok(value)
    }

    fn write_port_u8(&self, port: u16, value: u8) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.ops.push(PortOp::Write { port, value });
        if state.ibf_stuck {
            return Ok(());
        }
        match port {
            EC_COMMAND_PORT => state.write_command(value),
            EC_DATA_PORT => state.write_data(value),
            _ => return Err(format!("Mock EC refused non-ACPI port 0x{port:02X}")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_acpi_ec_ports_are_allowed() {
        assert!(is_acpi_ec_port(0x62));
        assert!(is_acpi_ec_port(0x66));
        assert!(!is_acpi_ec_port(0x00));
        assert!(!is_acpi_ec_port(0x31));
    }
}
