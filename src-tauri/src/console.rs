//! Release builds use the Windows GUI subsystem (no console window). For
//! `--bench-startup` we attach to the parent terminal so the timeline is visible.

#[cfg(windows)]
#[allow(unsafe_code)]
pub fn attach_parent() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: AttachConsole has no memory-safety preconditions; failure (no
    // parent console, or already attached) is harmless and ignored.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
pub fn attach_parent() {}
