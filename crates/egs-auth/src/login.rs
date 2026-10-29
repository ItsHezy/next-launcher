//! The login-page flow (docs/protocol/auth.md §1a).
//!
//! The user signs in on Epic's own page. When it reaches `/id/api/redirect`, the
//! init script below reads the JSON body and navigates to
//! `https://next-launcher.invalid/auth?...`. The app's navigation handler matches
//! that URL with [`parse_callback`] and cancels the navigation, so the code never
//! leaves the machine.

use egs_api::oauth::LAUNCHER_CLIENT_ID;
use egs_api::Secret;

pub const CALLBACK_PREFIX: &str = "https://next-launcher.invalid/auth";
pub const LOGIN_PAGE: &str = "https://www.epicgames.com/id/login";
pub const LOGOUT_PAGE: &str = "https://www.epicgames.com/id/logout";

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `https://www.epicgames.com/id/login?redirectUrl=<…/id/api/redirect?clientId=…&responseType=code[&codeChallenge=…]>`
pub fn login_url(code_challenge: Option<&str>) -> String {
    let mut redirect = format!(
        "https://www.epicgames.com/id/api/redirect?clientId={LAUNCHER_CLIENT_ID}&responseType=code"
    );
    if let Some(c) = code_challenge {
        redirect.push_str(&format!("&codeChallenge={c}&codeChallengeMethod=S256"));
    }
    format!("{LOGIN_PAGE}?redirectUrl={}", encode(&redirect))
}

/// Runs on every document in the login window; acts only on Epic's redirect endpoint.
pub const LOGIN_INIT_SCRIPT: &str = r#"(function () {
  if (location.origin !== 'https://www.epicgames.com' || location.pathname !== '/id/api/redirect') return;
  var done = false;
  function send(q) { if (done) return; done = true; location.replace('https://next-launcher.invalid/auth?' + q); }
  function grab() {
    var text = document.body ? document.body.innerText : '';
    var pre = document.querySelector('pre');
    if (pre) text = pre.innerText;
    try {
      var j = JSON.parse(text);
      if (j && j.authorizationCode) send('code=' + encodeURIComponent(j.authorizationCode));
      else if (j && j.exchangeCode) send('exchange=' + encodeURIComponent(j.exchangeCode));
      else send('error=no_code');
    } catch (e) { send('error=unreadable'); }
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', grab); else grab();
})();"#;

/// What the login window reported.
#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    AuthorizationCode(Secret),
    ExchangeCode(Secret),
    Failed(String),
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `Some(..)` if `url` is our callback; `None` for any other navigation.
pub fn parse_callback(url: &str) -> Option<Callback> {
    let rest = url.strip_prefix(CALLBACK_PREFIX)?;
    let query = rest.strip_prefix('?').unwrap_or("");
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = decode(v);
        match k {
            "code" if !v.is_empty() => return Some(Callback::AuthorizationCode(Secret::new(v))),
            "exchange" if !v.is_empty() => return Some(Callback::ExchangeCode(Secret::new(v))),
            "error" => return Some(Callback::Failed(v)),
            _ => {}
        }
    }
    Some(Callback::Failed("empty".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_url_encodes_redirect() {
        let u = login_url(Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
        assert!(u.starts_with("https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode%26codeChallenge%3DE9Me"));
        assert!(u.ends_with("codeChallengeMethod%3DS256"));
        assert!(!login_url(None).contains("codeChallenge"));
    }

    #[test]
    fn parses_callbacks() {
        assert_eq!(
            parse_callback("https://next-launcher.invalid/auth?code=abc123"),
            Some(Callback::AuthorizationCode(Secret::new("abc123")))
        );
        assert_eq!(
            parse_callback("https://next-launcher.invalid/auth?exchange=ex%2D1"),
            Some(Callback::ExchangeCode(Secret::new("ex-1")))
        );
        assert_eq!(
            parse_callback("https://next-launcher.invalid/auth?error=no_code"),
            Some(Callback::Failed("no_code".into()))
        );
        assert_eq!(parse_callback("https://www.epicgames.com/id/login"), None);
        assert_eq!(
            parse_callback("https://next-launcher.invalid/auth"),
            Some(Callback::Failed("empty".into()))
        );
    }
}
