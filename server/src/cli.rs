use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;

use crate::config::ServerConfig;
use crate::diagnose::DiagnoseReport;
use crate::ec::PowerMode;
use crate::fan;
use crate::i18n::{t, Language};
use crate::session::{AppSession, FanSnapshot, StartOptions};

#[allow(dead_code)]
pub const EXIT_OK: i32 = 0;
pub const EXIT_RUNTIME: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_NOT_ADMIN: i32 = 3;
pub const EXIT_PAWNIO: i32 = 4;
pub const EXIT_UNSUPPORTED_HARDWARE: i32 = 5;
pub const EXIT_FIRMWARE: i32 = 6;

#[derive(Parser, Debug)]
#[command(name = "evox2ctl")]
#[command(about = "EVO-X2 / SU_AXB35 control")]
pub struct Args {
    #[arg(long, global = true)]
    pub json: bool,
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
    Fan {
        fan: Option<String>,
        action: Option<String>,
        spec: Option<String>,
        rampdown: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

pub struct CliError {
    pub message: String,
    pub code: i32,
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        let code = exit_code_from_error(&message);
        Self { message, code }
    }
}

impl From<&str> for CliError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
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

pub fn run(command: Command, json: bool) -> Result<(), CliError> {
    let language = current_language();
    match command {
        Command::Mode {
            power_mode,
            dry_run,
        } => cmd_mode(language, json, power_mode.as_deref(), dry_run),
        Command::Status => cmd_status(language, json),
        Command::Diagnose => cmd_diagnose(language, json),
        Command::Fan {
            fan,
            action,
            spec,
            rampdown,
            dry_run,
        } => cmd_fan(
            language,
            json,
            fan.as_deref(),
            action.as_deref(),
            spec.as_deref(),
            rampdown.as_deref(),
            dry_run,
        ),
    }
}

pub fn exit_code_from_error(error: &str) -> i32 {
    let lower = error.to_ascii_lowercase();
    if error.contains("Administrator") || lower.contains("administrator") {
        EXIT_NOT_ADMIN
    } else if lower.contains("pawnio") {
        EXIT_PAWNIO
    } else if lower.contains("unsupported ec firmware") {
        EXIT_FIRMWARE
    } else if lower.contains("unsupported hardware") || lower.contains("ec writes are disabled") {
        EXIT_UNSUPPORTED_HARDWARE
    } else {
        EXIT_RUNTIME
    }
}

fn current_language() -> Language {
    ServerConfig::load()
        .map(|config| config.language())
        .unwrap_or(Language::Zh)
}

fn cmd_mode(
    language: Language,
    json: bool,
    power_mode: Option<&str>,
    dry_run: bool,
) -> Result<(), CliError> {
    match power_mode {
        None => {
            let session = open_session()?;
            let mode = session.power_mode()?;
            if json {
                emit_json(&json!({ "power_mode": mode }))
            } else {
                println!("{mode}");
                Ok(())
            }
        }
        Some(mode) => {
            let parsed = PowerMode::from_name(mode).ok_or_else(|| CliError {
                message: t(language, "invalid_power_mode").to_string(),
                code: EXIT_USAGE,
            })?;
            if dry_run {
                if json {
                    return emit_json(&json!({
                        "dry_run": true,
                        "power_mode": parsed.as_str(),
                        "register": "0x31",
                        "value": format!("0x{:02X}", parsed.to_ec_value()),
                    }));
                }
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
            let applied = session.set_power_mode(mode)?;
            if json {
                emit_json(&json!({ "power_mode": applied }))
            } else {
                println!("{applied}");
                Ok(())
            }
        }
    }
}

fn cmd_status(language: Language, json: bool) -> Result<(), CliError> {
    let session = open_session()?;
    let metrics = session.metrics()?;
    if json {
        return emit_json(&StatusOut {
            hardware: session.runtime.hardware.clone(),
            power_mode: metrics.power_mode.clone(),
            temperature: metrics.temperature,
            temperature_source: metrics.temperature_source.i18n_key().to_string(),
            fans: [
                fan_out(1, &metrics.fans[0]),
                fan_out(2, &metrics.fans[1]),
                fan_out(3, &metrics.fans[2]),
            ],
            pawnio: session.runtime.pawnio.clone(),
            secure_boot: session.runtime.secure_boot.clone(),
        });
    }
    println!("{} {}", t(language, "model"), session.runtime.hardware);
    println!(
        "{} {}",
        t(language, "power_mode_status"),
        t(language, &metrics.power_mode)
    );
    println!(
        "{} {} C ({})",
        t(language, "apu_temp"),
        metrics.temperature,
        t(language, metrics.temperature_source.i18n_key())
    );
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

fn cmd_diagnose(language: Language, json: bool) -> Result<(), CliError> {
    let session = open_session().ok();
    let report = DiagnoseReport::collect(session.as_ref());
    if json {
        emit_json(&report)
    } else {
        println!("{}", report.format_text(language));
        Ok(())
    }
}

fn cmd_fan(
    language: Language,
    json: bool,
    fan: Option<&str>,
    action: Option<&str>,
    spec: Option<&str>,
    rampdown: Option<&str>,
    dry_run: bool,
) -> Result<(), CliError> {
    match (fan, action, spec, rampdown) {
        (None, None, None, None) => {
            let session = open_session()?;
            let metrics = session.metrics()?;
            print_fans(language, json, &metrics.fans, None)
        }
        (Some(fan_spec), None, None, None) => {
            let fan_id = parse_fan_or_usage(fan_spec)?;
            let session = open_session()?;
            let snapshot = session.fan_snapshot(fan_id)?;
            print_fans(
                language,
                json,
                std::slice::from_ref(&snapshot),
                Some(fan_id),
            )
        }
        (Some(fan_spec), Some(mode), spec, rampdown) => {
            let fan_id = parse_fan_or_usage(fan_spec)?;
            apply_fan_command(language, json, fan_id, mode, spec, rampdown, dry_run)
        }
        _ => Err(usage_error(language)),
    }
}

fn apply_fan_command(
    language: Language,
    json: bool,
    fan_id: u8,
    mode: &str,
    spec: Option<&str>,
    rampdown: Option<&str>,
    dry_run: bool,
) -> Result<(), CliError> {
    let mode = mode.to_ascii_lowercase();
    let (level, rampup, rampdown_curve) = match mode.as_str() {
        "auto" if spec.is_none() && rampdown.is_none() => (0, None, None),
        "fixed" => {
            let level = spec
                .ok_or_else(|| usage_error(language))?
                .parse::<u8>()
                .map_err(|_| usage_error(language))?;
            if level > 5 || rampdown.is_some() {
                return Err(usage_error(language));
            }
            (level, None, None)
        }
        "curve" => {
            let rampup = fan::parse_curve(spec.ok_or_else(|| usage_error(language))?).map_err(
                |message| CliError {
                    message,
                    code: EXIT_USAGE,
                },
            )?;
            let rampdown = fan::parse_curve(rampdown.ok_or_else(|| usage_error(language))?)
                .map_err(|message| CliError {
                    message,
                    code: EXIT_USAGE,
                })?;
            (0, Some(rampup), Some(rampdown))
        }
        _ => return Err(usage_error(language)),
    };

    if dry_run {
        if json {
            return emit_json(&json!({
                "dry_run": true,
                "fan": fan_id,
                "name": fan::alias(fan_id),
                "mode": mode,
                "level": level,
                "rampup_curve": rampup,
                "rampdown_curve": rampdown_curve,
            }));
        }
        println!(
            "{} {} {mode} {}",
            t(language, "would_set_pmode"),
            t(language, fan::cli_label_key(fan_id)),
            level
        );
        return Ok(());
    }

    let session = open_session()?;
    session.apply_fan_edit(fan_id, &mode, level, rampup, rampdown_curve)?;
    let snapshot = session.fan_snapshot(fan_id)?;
    print_fans(
        language,
        json,
        std::slice::from_ref(&snapshot),
        Some(fan_id),
    )
}

fn print_fans(
    language: Language,
    json: bool,
    fans: &[FanSnapshot],
    only: Option<u8>,
) -> Result<(), CliError> {
    let ids: Vec<u8> = match only {
        Some(id) => vec![id],
        None => vec![1, 2, 3],
    };
    if json {
        let payload: Vec<FanOut> = ids
            .iter()
            .map(|id| {
                let snapshot = if only.is_some() {
                    &fans[0]
                } else {
                    &fans[(*id as usize) - 1]
                };
                fan_out(*id, snapshot)
            })
            .collect();
        return if only.is_some() {
            emit_json(&payload[0])
        } else {
            emit_json(&json!({ "fans": payload }))
        };
    }
    for id in ids {
        let snapshot = if only.is_some() {
            &fans[0]
        } else {
            &fans[(id as usize) - 1]
        };
        println!(
            "{} {} {} {} RPM",
            t(language, fan::cli_label_key(id)),
            snapshot.mode,
            snapshot.level,
            snapshot.rpm
        );
    }
    Ok(())
}

fn parse_fan_or_usage(spec: &str) -> Result<u8, CliError> {
    fan::parse_fan_id(spec).map_err(|message| CliError {
        message,
        code: EXIT_USAGE,
    })
}

fn usage_error(language: Language) -> CliError {
    CliError {
        message: t(language, "usage_cli").to_string(),
        code: EXIT_USAGE,
    }
}

fn fan_out(id: u8, snapshot: &FanSnapshot) -> FanOut {
    FanOut {
        id,
        name: fan::alias(id),
        mode: snapshot.mode.clone(),
        level: snapshot.level,
        rpm: snapshot.rpm,
        rampup_curve: snapshot.rampup_curve,
        rampdown_curve: snapshot.rampdown_curve,
    }
}

#[derive(Serialize)]
struct StatusOut {
    hardware: String,
    power_mode: String,
    temperature: u8,
    temperature_source: String,
    fans: [FanOut; 3],
    pawnio: String,
    secure_boot: String,
}

#[derive(Serialize)]
struct FanOut {
    id: u8,
    name: &'static str,
    mode: String,
    level: u8,
    rpm: u16,
    rampup_curve: [u8; 5],
    rampdown_curve: [u8; 5],
}

fn emit_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn open_session() -> Result<AppSession, CliError> {
    AppSession::start(StartOptions {
        service_mode: true,
        exclusive: false,
        restore: false,
        quiet: true,
        monitor_curves: false,
    })
    .map_err(CliError::from)
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_error_strings_to_exit_codes() {
        assert_eq!(
            exit_code_from_error(
                "This application must be run as Administrator to access the EC through PawnIO."
            ),
            EXIT_NOT_ADMIN
        );
        assert_eq!(
            exit_code_from_error("PawnIO is required for hardware access."),
            EXIT_PAWNIO
        );
        assert_eq!(
            exit_code_from_error(
                "Unsupported hardware 'Example'. EC writes are disabled to avoid damaging unrelated machines."
            ),
            EXIT_UNSUPPORTED_HARDWARE
        );
        assert_eq!(
            exit_code_from_error(
                "Unsupported EC firmware 1.03. This software requires EC firmware 1.04 or higher."
            ),
            EXIT_FIRMWARE
        );
        assert_eq!(
            exit_code_from_error("EC timeout waiting for input buffer"),
            EXIT_RUNTIME
        );
        assert_eq!(EXIT_OK, 0);
        assert_ne!(EXIT_OK, EXIT_USAGE);
    }

    #[test]
    fn administrator_beats_pawnio_in_the_same_sentence() {
        assert_eq!(
            exit_code_from_error("Administrator rights required for PawnIO"),
            EXIT_NOT_ADMIN
        );
    }
}
