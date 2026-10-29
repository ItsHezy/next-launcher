//! Free space on the volume holding a path (install disk-space check), and user idle time.

use std::path::Path;

/// Bytes available to the current user on the volume of `path` (or its nearest existing parent).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let wide: Vec<u16> = probe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call; the
    // out-pointer refers to a live u64; the other outputs are optional (null).
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// Seconds since the last keyboard or mouse input (for "Update only when idle").
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn idle_secs() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a live, correctly sized LASTINPUTINFO.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    // SAFETY: plain call without arguments. Both counters wrap at the same 32-bit boundary.
    let now = unsafe { GetTickCount() };
    Some(u64::from(now.wrapping_sub(info.dwTime)) / 1000)
}

#[cfg(not(windows))]
pub fn idle_secs() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_free_space_for_existing_and_missing_paths() {
        let here = std::env::temp_dir();
        let free = super::free_bytes(&here).unwrap_or(0);
        assert!(free > 0);
        assert!(super::free_bytes(&here.join("does/not/exist/yet")).is_some());
    }
}
