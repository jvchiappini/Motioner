//! Console attachment for the desktop binary.
//!
//! `motioner.exe` is built with the Windows GUI subsystem so that
//! double-clicking it (and the `--start` desktop launch) never opens a console
//! window next to the editor. The same binary is the CLI, so when it is started
//! from a terminal it must still print its result to that terminal and return
//! its exit code: [`attach_parent_console`] attaches to the parent console
//! before anything else runs.
//!
//! Handles the shell already handed to the process (a console, a pipe or a
//! redirected file) are valid and are never replaced; only a missing handle is
//! opened against the freshly attached console. A shell does not wait for a
//! GUI-subsystem executable unless it is started with `start /wait` or through
//! a script that waits on the child (like `scripts/startup_bench.py`), so the
//! prompt can return before the command finishes; that trade-off is documented
//! in `docs/reference/REF-0002.md`.

/// Attaches the process to the console of the parent, when there is one.
///
/// A no-op on platforms without a Win32 console and on a double-click (there is
/// no parent console to attach to).
pub fn attach_parent_console() {
    #[cfg(windows)]
    windows::attach_parent_console();
}

#[cfg(windows)]
#[allow(unsafe_code)] // raw Win32 FFI: AttachConsole/GetStdHandle/SetStdHandle
mod windows {
    use std::ffi::c_void;

    /// `AttachConsole` argument that selects the parent process console.
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    /// `GetStdHandle`/`SetStdHandle` selector for standard input (`-10`).
    const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6;
    /// `GetStdHandle`/`SetStdHandle` selector for standard output (`-11`).
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    /// `GetStdHandle`/`SetStdHandle` selector for standard error (`-12`).
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    /// The value `GetStdHandle` returns when a handle is missing.
    const INVALID_HANDLE_VALUE: isize = -1;
    /// `CreateFileW` access bits.
    const GENERIC_READ: u32 = 0x8000_0000;
    /// `CreateFileW` access bits.
    const GENERIC_WRITE: u32 = 0x4000_0000;
    /// `CreateFileW`: other processes may read the console.
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    /// `CreateFileW`: other processes may write the console.
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    /// `CreateFileW`: open an existing device.
    const OPEN_EXISTING: u32 = 3;

    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(std_handle: u32) -> isize;
        fn SetStdHandle(std_handle: u32, handle: isize) -> i32;
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *const c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: isize,
        ) -> isize;
    }

    /// Calls `AttachConsole` and opens any standard handle the process lacks.
    pub(super) fn attach_parent_console() {
        // SAFETY: the call takes a constant and no pointers; a zero result means
        // there is no parent console (double-click), which is not an error.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }
        // A handle inherited from the shell (terminal, pipe or redirected file)
        // is already valid and must not be replaced; only a missing one is
        // opened against the attached console.
        open_if_missing(STD_OUTPUT_HANDLE, "CONOUT$", GENERIC_WRITE);
        open_if_missing(STD_ERROR_HANDLE, "CONOUT$", GENERIC_WRITE);
        open_if_missing(STD_INPUT_HANDLE, "CONIN$", GENERIC_READ | GENERIC_WRITE);
    }

    /// Opens `device` and installs it as `std_handle` when it is not set.
    fn open_if_missing(std_handle: u32, device: &str, access: u32) {
        // SAFETY: `GetStdHandle` only reads the process handle table.
        let current = unsafe { GetStdHandle(std_handle) };
        if current != 0 && current != INVALID_HANDLE_VALUE {
            return;
        }
        let mut name: Vec<u16> = device.encode_utf16().collect();
        name.push(0);
        // SAFETY: `name` is a NUL-terminated UTF-16 string that lives for the
        // call; the remaining arguments are constants or null.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                0,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return;
        }
        // SAFETY: `handle` is owned by this process and stays open for its
        // lifetime, as the console device requires.
        unsafe {
            SetStdHandle(std_handle, handle);
        }
    }
}
