use clap::{Parser, Subcommand};

use crate::config::ServerConfig;
use crate::ec::PowerMode;
use crate::hardware::HardwareIdentity;
use crate::i18n::{t, Language};
use crate::pawnio;
use crate::platform::{is_admin, pawnio_install_version, secure_boot_status};
use crate::session::{AppSession, StartOptions};

#[derive(Parser, Debug)]
#[command(name = "evox2ctl")]
#[command(about = "EVO-X2 / SU_AXB35 control")]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Mode {
        power_mode: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    Status,
    Diagnose,
}

pub fn invoked_as_cli() -> bool {
    std::env::args_os()
        .next()
        .and_then(|path| {
            std::path::Path::new(&path)
                .file_stem()
                .map(|stem| stem.eq_ignore_ascii_case("evox2ctl"))
        })
        .unwrap_or(false)
}

pub fn run(command: Command) -> Result<(), String> {
    let language = current_language();
    match command {
        Command::Mode {
            power_mode,
            dry_run,
        } => cmd_mode(language, power_mode.as_deref(), dry_run),
        Command::Status => cmd_status(language),
        Command::Diagnose => cmd_diagnose(language),
    }
}

fn current_language() -> Language {
    ServerConfig::load()
        .map(|config| config.language())
        .unwrap_or(Language::Zh)
}

fn cmd_mode(language: Language, power_mode: Option<&str>, dry_run: bool) -> Result<(), String> {
    match power_mode {
        None => {
            let session = open_session()?;
            println!("{}", session.power_mode()?);
            Ok(())
        }
        Some(mode) => {
            let parsed = PowerMode::from_name(mode)
                .ok_or_else(|| t(language, "invalid_power_mode").to_string())?;
            if dry_run {
                println!("{}", t(language, "would_set_pmode"));
                println!("{} 0x31", t(language, "register"));
                println!(
                    "{}    0x{:02X}",
                    t(language, "value_label"),
                    parsed.to_ec_value()
                );
                println!(
                    "{}     {}",
                    t(language, "mode_name"),
                    t(language, parsed.as_str())
                );
                return Ok(());
            }
            let session = open_session()?;
            println!("{}", session.set_power_mode(mode)?);
            Ok(())
        }
    }
}

fn cmd_status(language: Language) -> Result<(), String> {
    let session = open_session()?;
    let metrics = session.metrics()?;
    println!("{} {}", t(language, "model"), session.runtime.hardware);
    println!(
        "{} {}",
        t(language, "power_mode_status"),
        t(language, &metrics.power_mode)
    );
    println!("{} {} C", t(language, "apu_temp"), metrics.temperature);
    println!("{} {} RPM", t(language, "cpu_fan"), metrics.fans[0].rpm);
    println!(
        "{} {} RPM",
        t(language, "secondary_fan"),
        metrics.fans[1].rpm
    );
    println!("{} {} RPM", t(language, "system_fan"), metrics.fans[2].rpm);
    println!(
        "{} {}",
        t(language, "pawnio"),
        title_case(&session.runtime.pawnio)
    );
    println!(
        "{} {}",
        t(language, "secure_boot"),
        session.runtime.secure_boot
    );
    Ok(())
}

fn cmd_diagnose(language: Language) -> Result<(), String> {
    println!("{} Windows ({})", t(language, "os"), std::env::consts::OS);
    println!("{} {}", t(language, "architecture"), std::env::consts::ARCH);
    println!(
        "{} {}",
        t(language, "admin_status"),
        if is_admin() {
            t(language, "administrator")
        } else {
            t(language, "standard_user")
        }
    );
    println!(
        "{} {}",
        t(language, "pawnio_status"),
        if pawnio::probe_device_present() {
            match pawnio_install_version() {
                Some(version) => format!("{}{version})", t(language, "pawnio_connected_ver")),
                None => t(language, "pawnio_connected").to_string(),
            }
        } else {
            t(language, "pawnio_unavailable").to_string()
        }
    );

    let session = open_session().ok();
    println!(
        "{} {}",
        t(language, "lpcacpiec_status"),
        session
            .as_ref()
            .map(|item| item.runtime.lpcacpiec.clone())
            .unwrap_or_else(|| t(language, "unknown").to_string())
    );
    println!(
        "{} {}",
        t(language, "motherboard"),
        session
            .as_ref()
            .map(|item| item.runtime.hardware.clone())
            .unwrap_or_else(|| HardwareIdentity::detect().summary())
    );
    println!(
        "{} {}",
        t(language, "firmware_status"),
        session
            .as_ref()
            .and_then(|item| item.firmware_version().ok())
            .unwrap_or_else(|| t(language, "unknown").to_string())
    );
    println!(
        "{} {}",
        t(language, "current_pmode"),
        session
            .as_ref()
            .and_then(|item| item.power_mode().ok())
            .unwrap_or_else(|| t(language, "unknown").to_string())
    );
    println!(
        "{} {}",
        t(language, "secure_boot_status"),
        secure_boot_status()
    );
    if let Some(session) = session {
        if !session.runtime.hardware_supported {
            println!("{}", t(language, "hardware_unsupported"));
        }
    }
    Ok(())
}

fn open_session() -> Result<AppSession, String> {
    AppSession::start(StartOptions {
        service_mode: true,
        exclusive: false,
        restore: false,
        quiet: true,
        monitor_curves: false,
    })
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
