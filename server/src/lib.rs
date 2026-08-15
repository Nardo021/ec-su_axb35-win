use std::ptr;
use std::sync::Arc;

use clap::Parser;
use winapi::um::consoleapi::AllocConsole;
use winapi::um::shellapi::ShellExecuteW;
use winapi::um::wincon::{AttachConsole, ATTACH_PARENT_PROCESS};
use winapi::um::winuser::{MessageBoxW, SW_SHOWNORMAL};

mod alert;
mod cli;
mod config;
mod diagnose;
mod ec;
mod ec_io;
mod fan;
mod gui;
mod hardware;
mod i18n;
mod logger;
mod pawnio;
mod platform;
mod session;
mod thermal;
mod tray;

use cli::Args;
use config::ServerConfig;
use i18n::{t, Language};
use platform::ALREADY_RUNNING;
use session::{AppSession, StartOptions};

pub fn entry() {
    let args = Args::parse();

    if let Some(command) = args.command {
        attach_cli_console();
        if let Err(error) = cli::run(command, args.json) {
            emit_cli_error(args.json, &error);
            std::process::exit(error.code);
        }
        return;
    }

    if cli::invoked_as_cli() {
        attach_cli_console();
        let language = current_language();
        emit_cli_error(
            args.json,
            &cli::CliError {
                message: t(language, "usage_cli").to_string(),
                code: cli::EXIT_USAGE,
            },
        );
        std::process::exit(cli::EXIT_USAGE);
    }

    match AppSession::start(StartOptions {
        service_mode: false,
        exclusive: true,
        restore: true,
        quiet: false,
        monitor_curves: true,
    }) {
        Ok(session) => {
            let session = Arc::new(session);
            sync_start_with_windows(&session);
            let tray_rx = tray::start(Arc::clone(&session));
            let result = gui::run(Arc::clone(&session), tray_rx);
            tray::shutdown();
            session.shutdown();
            if let Err(error) = result {
                show_startup_error(&error);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        Err(error) if error == ALREADY_RUNNING => {
            let _ = tray::request_show();
            std::process::exit(0);
        }
        Err(error) => {
            show_startup_error(&error);
            std::process::exit(1);
        }
    }
}

fn attach_cli_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            AllocConsole();
        }
    }
}

fn emit_cli_error(json: bool, error: &cli::CliError) {
    if json {
        eprintln!(
            "{}",
            serde_json::json!({ "error": error.message, "code": error.code })
        );
    } else {
        eprintln!("{}", error.message);
    }
}

fn current_language() -> Language {
    ServerConfig::load()
        .map(|config| config.language())
        .unwrap_or(Language::En)
}

fn sync_start_with_windows(session: &AppSession) {
    let actual = platform::is_start_with_windows();
    let mut config = session.config.lock().unwrap();
    if config.start_with_windows == actual {
        return;
    }
    config.start_with_windows = actual;
    if let Err(error) = config.save() {
        session
            .logger
            .lock()
            .unwrap()
            .error(&format!("Failed to sync start-with-Windows: {error}"));
    }
}

fn show_startup_error(error: &str) {
    let language = current_language();
    let title = t(language, "app_title");
    let pawnio = error.to_ascii_lowercase().contains("pawnio");
    let message = if error.contains("Administrator") {
        t(language, "admin_required").to_string()
    } else if pawnio {
        t(language, "pawnio_missing").to_string()
    } else {
        error.to_string()
    };

    eprintln!("Error: {message}");
    show_message(title, &message);
    if pawnio {
        open_url("https://pawnio.eu/");
    }
}

fn show_message(title: &str, message: &str) {
    let title_w = wide(title);
    let message_w = wide(message);
    unsafe {
        MessageBoxW(ptr::null_mut(), message_w.as_ptr(), title_w.as_ptr(), 0x10);
    }
}

fn open_url(url: &str) {
    let operation = wide("open");
    let url_w = wide(url);
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            url_w.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
