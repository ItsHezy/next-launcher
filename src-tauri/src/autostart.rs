//! "Run at startup" (Phase 9): one value under the current user's
//! `Software\Microsoft\Windows\CurrentVersion\Run` key, written only when the user turns the
//! setting on and removed when they turn it off. No admin rights, no scheduled tasks.

use tauri::{AppHandle, Manager};

use crate::commands::{settings_value, AppState};
use crate::install::DownloadsState;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Development builds (with the install guard) use their own entry name.
fn value_name(app: &AppHandle) -> &'static str {
    let dev = app
        .try_state::<DownloadsState>()
        .is_some_and(|d| d.install_root.is_some());
    if dev {
        "Next Launcher (dev)"
    } else {
        "Next Launcher"
    }
}

/// `"C:\...\next-launcher.exe" --autostart`
fn command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --autostart", exe.display()))
}

/// Apply the setting: write or remove the Run value. Returns whether it is now registered.
pub fn apply(app: &AppHandle) -> bool {
    let on = app
        .try_state::<AppState>()
        .and_then(|s| settings_value(&s.cache, "general.runAtStartup"))
        .is_some_and(|v| v == "true");
    let name = value_name(app);
    let result = if on {
        match command() {
            Some(cmd) if registry::read(RUN_KEY, name).as_deref() != Some(cmd.as_str()) => {
                registry::write(RUN_KEY, name, &cmd)
            }
            Some(_) => Ok(()), // already correct
            None => Err("no exe path".into()),
        }
    } else if registry::read(RUN_KEY, name).is_some() {
        registry::delete(RUN_KEY, name)
    } else {
        Ok(())
    };
    match result {
        Ok(()) => {
            tracing::info!(on, "run at startup applied");
            on
        }
        Err(e) => {
            tracing::warn!(error = %e, "run at startup not applied");
            false
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod registry {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn read(key: &str, name: &str) -> Option<String> {
        let (k, n) = (wide(key), wide(name));
        let mut buf = vec![0u16; 2048];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: NUL-terminated key/value names; `buf` holds `bytes` bytes; both outlive the call.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                k.as_ptr(),
                n.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let len = (bytes as usize / 2).saturating_sub(1); // minus the terminating NUL
        Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
    }

    pub fn write(key: &str, name: &str, value: &str) -> Result<(), String> {
        let (k, n, v) = (wide(key), wide(name), wide(value));
        // SAFETY: NUL-terminated strings that outlive the call; size in bytes includes the NUL.
        let rc = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                k.as_ptr(),
                n.as_ptr(),
                REG_SZ,
                v.as_ptr().cast(),
                (v.len() * 2) as u32,
            )
        };
        (rc == ERROR_SUCCESS)
            .then_some(())
            .ok_or_else(|| format!("error {rc}"))
    }

    pub fn delete(key: &str, name: &str) -> Result<(), String> {
        let (k, n) = (wide(key), wide(name));
        // SAFETY: NUL-terminated strings that outlive the call.
        let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
        (rc == ERROR_SUCCESS)
            .then_some(())
            .ok_or_else(|| format!("error {rc}"))
    }
}

#[cfg(not(windows))]
mod registry {
    pub fn read(_: &str, _: &str) -> Option<String> {
        None
    }
    pub fn write(_: &str, _: &str, _: &str) -> Result<(), String> {
        Err("unsupported".into())
    }
    pub fn delete(_: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
}
