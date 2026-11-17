//! Console-subsystem companion for terminals.
//!
//! `motioner.exe` is a GUI-subsystem binary: `cmd` treats it as asynchronous,
//! does not wait for it and does not hand it `>` redirection handles, so a
//! terminal command line loses its output file and `%ERRORLEVEL%`. This binary
//! runs the same [`motioner_app::run_cli`] with the console subsystem, so
//! `motioner-cli render ... > out.txt` and `$LASTEXITCODE` work in `cmd` and
//! PowerShell. Invoking `motioner` still uses the desktop binary; copy this
//! file to `motioner.com` next to `motioner.exe` when the console behaviour is
//! wanted under the same name (Windows resolves `.com` before `.exe`).
//!
//! Built from the same crate and library, so there is no duplicated CLI code.

fn main() -> std::process::ExitCode {
    motioner_app::run_cli()
}
