#![cfg_attr(windows, windows_subsystem = "windows")]
//! GUI-subsystem entry point (the desktop app and editor).
//!
//! Built as a Windows GUI executable so double-clicking `motioner.exe` never
//! opens a console window next to the editor. The CLI logic lives in the
//! library ([`motioner_app::run_cli`]); `motioner-cli.exe` is the
//! console-subsystem companion for terminals (prompt 17 S6).

fn main() -> std::process::ExitCode {
    motioner_app::run_cli()
}
