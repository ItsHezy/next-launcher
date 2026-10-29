//! Start a game and tell when it, and anything it started, has exited
//! (docs/protocol/launch.md "Process tracking").
//!
//! No hooks, no job objects and no handles beyond `PROCESS_QUERY_LIMITED_INFORMATION`,
//! so anti-cheat launchers are left alone. Polled by the app every 2 s, only while a
//! game is running.
#![allow(unsafe_code)]

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessId, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
    INFINITE, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::args::{Launch, LaunchError};

const ERROR_ELEVATION_REQUIRED: i32 = 740;

/// A started game process.
pub struct Started {
    pub pid: u32,
    /// The executable asked for administrator rights, so Windows showed its UAC prompt.
    pub elevated: bool,
    child: Option<Child>,
}

/// Start exactly the manifest's executable with exactly these arguments. Programs
/// whose manifest requires administrator rights go through `ShellExecuteExW`, so
/// Windows shows its own UAC prompt; nothing is ever elevated silently.
pub fn start(l: &Launch) -> Result<Started, LaunchError> {
    let spawned = Command::new(&l.exe)
        .args(&l.args)
        .current_dir(&l.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(child) => Ok(Started {
            pid: child.id(),
            elevated: false,
            child: Some(child),
        }),
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => {
            let params = command_line(l.args.iter().map(|a| a.as_os_str()));
            let (pid, _) = shell_execute("runas", l.exe.as_os_str(), &params, Some(&l.cwd), false)?;
            Ok(Started {
                pid,
                elevated: true,
                child: None,
            })
        }
        Err(e) => Err(LaunchError::Spawn(e.to_string())),
    }
}

/// Open a URL or file with its registered handler (third-party store links).
pub fn open_with_shell(target: &str) -> Result<(), LaunchError> {
    shell_execute("open", OsStr::new(target), OsStr::new(""), None, false).map(|_| ())
}

/// Run a program and wait for it (prerequisite installers). Windows shows UAC if the
/// installer asks for it. Returns the exit code.
pub fn run_and_wait(exe: &Path, args: &str, cwd: &Path) -> Result<u32, LaunchError> {
    let (_, code) = shell_execute("open", exe.as_os_str(), OsStr::new(args), Some(cwd), true)?;
    code.ok_or_else(|| LaunchError::Spawn("no exit code".into()))
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn shell_execute(
    verb: &str,
    file: &OsStr,
    params: &OsStr,
    dir: Option<&Path>,
    wait: bool,
) -> Result<(u32, Option<u32>), LaunchError> {
    let verb_w = wide(OsStr::new(verb));
    let file_w = wide(file);
    let params_w = wide(params);
    let dir_w = dir.map(|d| wide(d.as_os_str()));
    // SAFETY: an all-zero SHELLEXECUTEINFOW is valid (null pointers, zero flags); the
    // fields the call reads are set below.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb_w.as_ptr();
    info.lpFile = file_w.as_ptr();
    info.lpParameters = params_w.as_ptr();
    info.lpDirectory = dir_w.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: every string is NUL-terminated UTF-16 that outlives the call, and `info`
    // is a live, correctly sized struct.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // ERROR_CANCELLED (1223) when the user declines the UAC prompt.
        return Err(LaunchError::Spawn(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let h = info.hProcess;
    if h.is_null() {
        return Ok((0, None)); // handled by an already-running program (e.g. a URL)
    }
    // SAFETY: `h` is the process handle ShellExecuteExW gave us; we close it below.
    let pid = unsafe { GetProcessId(h) };
    let mut code = None;
    if wait {
        let mut c: u32 = 0;
        // SAFETY: as above; `c` is a live u32 for the out-parameter.
        unsafe {
            WaitForSingleObject(h, INFINITE);
            if GetExitCodeProcess(h, &mut c) != 0 {
                code = Some(c);
            }
        }
    }
    // SAFETY: closing the handle we own exactly once.
    unsafe { CloseHandle(h) };
    Ok((pid, code))
}

/// Join arguments into one Windows command line (the quoting `CommandLineToArgvW` undoes).
pub fn command_line<'a>(args: impl IntoIterator<Item = &'a OsStr>) -> std::ffi::OsString {
    let mut out = String::new();
    for (i, a) in args.into_iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let a = a.to_string_lossy();
        if !a.is_empty() && !a.contains([' ', '\t', '"']) {
            out.push_str(&a);
            continue;
        }
        out.push('"');
        let mut backslashes = 0;
        for c in a.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                c => {
                    out.extend(std::iter::repeat_n('\\', backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
    }
    out.into()
}

struct Proc {
    pid: u32,
    ppid: u32,
    name: String,
}

fn snapshot() -> Vec<Proc> {
    // SAFETY: plain Win32 call; the handle is closed below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: an all-zero PROCESSENTRY32W is valid; `dwSize` is set as the API requires.
    let mut e: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `snap` is a valid snapshot handle and `e` a live, sized entry.
    let mut ok = unsafe { Process32FirstW(snap, &mut e) };
    while ok != 0 {
        let len = e
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(e.szExeFile.len());
        out.push(Proc {
            pid: e.th32ProcessID,
            ppid: e.th32ParentProcessID,
            name: String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase(),
        });
        // SAFETY: as above.
        ok = unsafe { Process32NextW(snap, &mut e) };
    }
    // SAFETY: closing the snapshot handle we own exactly once.
    unsafe { CloseHandle(snap) };
    out
}

/// Full image path of a process, with query-only access.
fn image_path(pid: u32) -> Option<String> {
    // SAFETY: query-only access; the handle is closed below.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` u16s and both outlive the call; flags 0 = Win32 path.
    let ok = unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) };
    // SAFETY: closing the handle we own exactly once.
    unsafe { CloseHandle(h) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// Tracks one launched game: the started process, everything it starts, and any process
/// running from the game's folder (bootstrappers that start the real game and exit).
pub struct Tracker {
    dir: String,
    names: HashSet<String>,
    child: Option<Child>,
    alive: HashSet<u32>,
    seen: HashSet<u32>,
}

impl Tracker {
    /// `exe_names`: file names of the executables in the game's manifest.
    pub fn new(
        install_dir: &Path,
        exe_names: impl IntoIterator<Item = String>,
        started: Option<Started>,
    ) -> Self {
        let mut dir = install_dir
            .to_string_lossy()
            .to_lowercase()
            .replace('/', "\\");
        if !dir.ends_with('\\') {
            dir.push('\\');
        }
        let mut t = Self {
            dir,
            names: exe_names.into_iter().map(|n| n.to_lowercase()).collect(),
            child: None,
            alive: HashSet::new(),
            seen: HashSet::new(),
        };
        if let Some(s) = started {
            if s.pid != 0 {
                t.alive.insert(s.pid);
                t.seen.insert(s.pid);
            }
            t.child = s.child;
        }
        t
    }

    /// One look at the process list: true while any part of the game is running.
    pub fn poll(&mut self) -> bool {
        let child_alive = self
            .child
            .as_mut()
            .is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        let procs = snapshot();
        let live: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        self.alive.retain(|p| live.contains(p));
        // Everything started by a process we tracked, repeated for grandchildren.
        loop {
            let before = self.alive.len();
            for p in &procs {
                if p.pid != p.ppid && !self.alive.contains(&p.pid) && self.seen.contains(&p.ppid) {
                    self.alive.insert(p.pid);
                    self.seen.insert(p.pid);
                }
            }
            if self.alive.len() == before {
                break;
            }
        }
        // Game executables running from the install folder. Only processes whose name
        // matches are opened, and only to read their path.
        for p in &procs {
            if !self.alive.contains(&p.pid)
                && self.names.contains(&p.name)
                && image_path(p.pid).is_some_and(|i| i.to_lowercase().starts_with(&self.dir))
            {
                self.alive.insert(p.pid);
                self.seen.insert(p.pid);
            }
        }
        child_alive || !self.alive.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Run by copies of the test binary in the tests below.
    #[test]
    #[ignore]
    fn sleep_helper() {
        std::thread::sleep(Duration::from_millis(2500));
    }

    fn helper(exe: &Path) -> Child {
        Command::new(exe)
            .args(["--ignored", "--exact", "process::tests::sleep_helper"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn follows_children_after_the_parent_exits() {
        // cmd starts ping in the background and exits at once; ping runs ~2 s.
        let child = Command::new("cmd")
            .args(["/c", "start", "/b", "", "ping", "-n", "3", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Started {
            pid: child.id(),
            elevated: false,
            child: Some(child),
        };
        let mut t = Tracker::new(Path::new("C:\\nowhere"), [], Some(started));
        let t0 = Instant::now();
        let mut ran_without_parent = false;
        while t.poll() {
            let parent_done = t
                .child
                .as_mut()
                .is_some_and(|c| matches!(c.try_wait(), Ok(Some(_))));
            ran_without_parent |= parent_done;
            assert!(t0.elapsed() < Duration::from_secs(20), "never stopped");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(ran_without_parent, "the child kept the game 'running'");
        assert!(
            t0.elapsed() >= Duration::from_millis(1500),
            "{:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn finds_game_processes_by_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("My Game");
        std::fs::create_dir_all(game.join("bin")).unwrap();
        let exe = game.join("bin").join("MyGame.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let mut p = helper(&exe);

        let mut other = Tracker::new(&tmp.path().join("Other"), ["mygame.exe".to_owned()], None);
        let mut t = Tracker::new(&game, ["MyGame.exe".to_owned()], None);
        assert!(t.poll(), "running from the game folder");
        assert!(!other.poll(), "a different folder doesn't match");
        p.wait().unwrap();
        assert!(!t.poll());
    }

    #[test]
    fn command_line_round_trips() {
        let args = [
            "plain",
            "with space",
            r#"q"uote"#,
            r"trail\",
            "",
            r#"-k=C:\a b\"#,
        ];
        let line = command_line(args.iter().map(OsStr::new));
        assert_eq!(
            crate::args::split_args(&line.to_string_lossy()),
            args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
    }
}
