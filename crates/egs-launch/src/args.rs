//! Launch command line, in the order EGL uses (docs/protocol/launch.md "Argument order").

use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Single-use exchange code for `-AUTH_PASSWORD`. `Debug` never shows it.
#[derive(Clone)]
pub struct ExchangeCode(pub String);

impl fmt::Debug for ExchangeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExchangeCode(<redacted>)")
    }
}

#[derive(Debug, Clone)]
pub struct LaunchSpec<'a> {
    pub install_dir: &'a Path,
    /// Manifest `launch_exe`, relative to the install folder.
    pub launch_exe: &'a str,
    /// Manifest `launch_command`.
    pub launch_command: &'a str,
    /// Catalog `customAttributes.AdditionalCommandLine`.
    pub additional_command_line: &'a str,
    pub app_name: &'a str,
    pub namespace: &'a str,
    pub display_name: &'a str,
    pub account_id: &'a str,
    /// e.g. `en-US`.
    pub locale: &'a str,
    /// `None` = offline launch (`-AUTH_PASSWORD=` stays empty).
    pub exchange_code: Option<&'a ExchangeCode>,
    /// Ownership token file, only for titles that require one.
    pub ownership_token: Option<&'a Path>,
    pub deployment_id: Option<&'a str>,
    /// The user's own extra arguments (Settings → Manage games).
    pub user_args: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub exe: PathBuf,
    pub args: Vec<OsString>,
    /// The folder holding the executable.
    pub cwd: PathBuf,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LaunchError {
    #[error("this game's files don't say which program to start")]
    NoExecutable,
    #[error("unsafe executable path in the manifest: {0}")]
    UnsafePath(String),
    #[error("couldn't start the game: {0}")]
    Spawn(String),
}

pub fn build(spec: &LaunchSpec<'_>) -> Result<Launch, LaunchError> {
    let rel = spec.launch_exe.trim();
    if rel.is_empty() {
        return Err(LaunchError::NoExecutable);
    }
    let mut exe = spec.install_dir.to_path_buf();
    for c in Path::new(&rel.replace('/', "\\")).components() {
        match c {
            Component::Normal(p) => exe.push(p),
            Component::CurDir => {}
            _ => return Err(LaunchError::UnsafePath(rel.to_owned())),
        }
    }
    let cwd = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| spec.install_dir.to_path_buf());

    let mut args: Vec<OsString> = Vec::new();
    let mut push = |s: String| args.push(OsString::from(s));
    split_args(spec.launch_command)
        .into_iter()
        .for_each(&mut push);
    split_args(spec.additional_command_line)
        .into_iter()
        .for_each(&mut push);
    push("-AUTH_LOGIN=unused".into());
    push(format!(
        "-AUTH_PASSWORD={}",
        spec.exchange_code.map(|c| c.0.as_str()).unwrap_or("")
    ));
    push("-AUTH_TYPE=exchangecode".into());
    push(format!("-epicapp={}", spec.app_name));
    push("-epicenv=Prod".into());
    if let Some(ovt) = spec.ownership_token {
        push(format!("-epicovt={}", ovt.display()));
    }
    push("-EpicPortal".into());
    push(format!("-epicusername={}", spec.display_name));
    push(format!("-epicuserid={}", spec.account_id));
    push(format!("-epiclocale={}", spec.locale));
    push(format!("-epicsandboxid={}", spec.namespace));
    if let Some(d) = spec.deployment_id.filter(|d| !d.is_empty()) {
        push(format!("-epicdeploymentid={d}"));
    }
    split_args(spec.user_args).into_iter().for_each(&mut push);
    Ok(Launch { exe, args, cwd })
}

/// Split a command line the way Windows programs do (`CommandLineToArgvW` rules):
/// whitespace separates, `"` groups, `2n` backslashes + `"` → `n` backslashes and a
/// quote toggle, `2n+1` → `n` backslashes and a literal `"`, `""` inside quotes → `"`.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_quotes, mut have) = (false, false);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                let start = i;
                while i < chars.len() && chars[i] == '\\' {
                    i += 1;
                }
                let n = i - start;
                if i < chars.len() && chars[i] == '"' {
                    cur.extend(std::iter::repeat_n('\\', n / 2));
                    if n % 2 == 1 {
                        cur.push('"');
                        i += 1;
                    }
                } else {
                    cur.extend(std::iter::repeat_n('\\', n));
                }
                have = true;
                continue;
            }
            '"' => {
                if in_quotes && chars.get(i + 1) == Some(&'"') {
                    cur.push('"');
                    i += 1;
                } else {
                    in_quotes = !in_quotes;
                }
                have = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if have {
                    out.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            c => {
                cur.push(c);
                have = true;
            }
        }
        i += 1;
    }
    if have {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>(code: Option<&'a ExchangeCode>) -> LaunchSpec<'a> {
        LaunchSpec {
            install_dir: Path::new("C:\\Games\\Wtl"),
            launch_exe: "Bin/Win64/wtl.exe",
            launch_command: "-obfuscationid=abc \"-map=My Map\"",
            additional_command_line: "-nosplash",
            app_name: "app1",
            namespace: "ns1",
            display_name: "Player One",
            account_id: "acc1",
            locale: "en-US",
            exchange_code: code,
            ownership_token: None,
            deployment_id: Some("dep1"),
            user_args: "-windowed -ResX=1280",
        }
    }

    fn strs(l: &Launch) -> Vec<String> {
        l.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn builds_egl_order_online() {
        let code = ExchangeCode("xyz".into());
        let l = build(&spec(Some(&code))).unwrap();
        assert_eq!(l.exe, PathBuf::from("C:\\Games\\Wtl\\Bin\\Win64\\wtl.exe"));
        assert_eq!(l.cwd, PathBuf::from("C:\\Games\\Wtl\\Bin\\Win64"));
        assert_eq!(
            strs(&l),
            [
                "-obfuscationid=abc",
                "-map=My Map",
                "-nosplash",
                "-AUTH_LOGIN=unused",
                "-AUTH_PASSWORD=xyz",
                "-AUTH_TYPE=exchangecode",
                "-epicapp=app1",
                "-epicenv=Prod",
                "-EpicPortal",
                "-epicusername=Player One",
                "-epicuserid=acc1",
                "-epiclocale=en-US",
                "-epicsandboxid=ns1",
                "-epicdeploymentid=dep1",
                "-windowed",
                "-ResX=1280",
            ]
        );
    }

    #[test]
    fn offline_and_ownership_token() {
        let ovt = PathBuf::from("C:\\data\\tmp\\ns1item.ovt");
        let mut s = spec(None);
        s.ownership_token = Some(&ovt);
        s.deployment_id = None;
        let a = strs(&build(&s).unwrap());
        assert!(a.contains(&"-AUTH_PASSWORD=".to_owned()));
        let ovt_at = a.iter().position(|x| x.starts_with("-epicovt=")).unwrap();
        let env_at = a.iter().position(|x| x == "-epicenv=Prod").unwrap();
        let portal_at = a.iter().position(|x| x == "-EpicPortal").unwrap();
        assert!(env_at < ovt_at && ovt_at < portal_at);
        assert!(!a.iter().any(|x| x.starts_with("-epicdeploymentid")));
    }

    #[test]
    fn refuses_missing_or_escaping_executables() {
        let mut s = spec(None);
        s.launch_exe = "";
        assert_eq!(build(&s), Err(LaunchError::NoExecutable));
        s.launch_exe = "..\\..\\Windows\\evil.exe";
        assert!(matches!(build(&s), Err(LaunchError::UnsafePath(_))));
        s.launch_exe = "C:\\Windows\\evil.exe";
        assert!(matches!(build(&s), Err(LaunchError::UnsafePath(_))));
    }

    #[test]
    fn exchange_code_is_not_in_debug_output() {
        let code = ExchangeCode("secret-code".into());
        assert!(!format!("{:?}", spec(Some(&code))).contains("secret-code"));
    }

    #[test]
    fn splits_like_windows() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("  a  b "), ["a", "b"]);
        assert_eq!(split_args(r#""a b" c"#), ["a b", "c"]);
        assert_eq!(split_args(r#"-k="v w""#), ["-k=v w"]);
        assert_eq!(split_args(r#"a\\\"b"#), [r#"a\"b"#]);
        assert_eq!(split_args(r#""a\\" b"#), [r"a\", "b"]);
        assert_eq!(split_args(r"C:\Path\To"), [r"C:\Path\To"]);
        assert_eq!(split_args(r#""say ""hi""""#), [r#"say "hi""#]);
        assert_eq!(split_args(r#""""#), [""]);
    }
}
