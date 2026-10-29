//! Log redaction (brief section 9: logs never contain tokens, cookies or email
//! addresses). Every formatted log line passes through [`redact`] before it is
//! written. Code should not log secrets in the first place; this is the backstop.

use std::io::{self, Write};
use std::sync::OnceLock;

use regex_lite::Regex;
use tracing_subscriber::fmt::MakeWriter;

struct Rule {
    re: Regex,
    with: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |pat: &str, with: &'static str| Rule {
            // Patterns are constants covered by tests; a bad one is a startup invariant.
            #[allow(clippy::expect_used)]
            re: Regex::new(pat).expect("valid redaction regex"),
            with,
        };
        vec![
            // Epic JWT tokens (access/refresh, eg1 and ep1).
            r(r"\b(eg1|ep1)~[A-Za-z0-9._\-]+", "${1}~<redacted>"),
            // Authorization headers.
            r(r"(?i)\b(authorization\s*[:=]\s*)(bearer|basic)\s+[A-Za-z0-9._~+/=\-]+", "${1}${2} <redacted>"),
            // JSON fields that carry secrets.
            r(
                r#""(access_token|refresh_token|token|code|authorizationCode|exchangeCode|exchange_code|code_verifier|signedTicket|sid|password|readLink|writeLink)"\s*:\s*"[^"]*""#,
                r#""${1}":"<redacted>""#,
            ),
            // Query-string / form parameters that carry secrets.
            r(
                r"\b(code|exchangeCode|exchange_code|refresh_token|access_token|token|code_verifier|codeChallenge|cf_token|sid|Signature|Policy|Key-Pair-Id)=[^&\s'\x22]+",
                "${1}=<redacted>",
            ),
            // Game launch auth argument.
            r(r"(-AUTH_PASSWORD=)[^\s'\x22]+", "${1}<redacted>"),
            // Email addresses.
            r(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}", "<email>"),
        ]
    })
}

/// Remove secrets and personal data from one log line.
pub fn redact(input: &str) -> String {
    let mut out = input.to_owned();
    for rule in rules() {
        if rule.re.is_match(&out) {
            out = rule.re.replace_all(&out, rule.with).into_owned();
        }
    }
    out
}

/// Wraps a `MakeWriter` so each event is buffered and redacted before writing.
pub struct Redacting<M>(pub M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Redacting<M> {
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.0.make_writer(),
            buf: Vec::with_capacity(256),
        }
    }
}

pub struct RedactingWriter<W: Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<W: Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let line = redact(&String::from_utf8_lossy(&self.buf));
        let _ = self.inner.write_all(line.as_bytes());
        let _ = self.inner.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_epic_tokens() {
        let s = "kill url .../sessions/kill/eg1~eyJTYW1wbGVIZWFkZXI.c2FtcGxl-DEF_1 done";
        assert_eq!(redact(s), "kill url .../sessions/kill/eg1~<redacted> done");
    }

    #[test]
    fn redacts_json_secret_fields() {
        let s = r#"{"access_token":"eg1~abc","refresh_token": "xyz","account_id":"a1","authorizationCode":"c0de"}"#;
        let r = redact(s);
        assert!(
            !r.contains("xyz") && !r.contains("c0de") && !r.contains("abc"),
            "{r}"
        );
        assert!(r.contains(r#""account_id":"a1""#));
    }

    #[test]
    fn redacts_query_params_and_headers() {
        let s = "GET https://x/y?code=0123456789abcdef&locale=en Authorization: bearer eg1~abc.def";
        let r = redact(s);
        assert!(r.contains("code=<redacted>"), "{r}");
        assert!(r.contains("locale=en"));
        assert!(!r.contains("abc.def"), "{r}");
        let s = "Authorization: Basic MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFh";
        assert_eq!(redact(s), "Authorization: Basic <redacted>");
    }

    #[test]
    fn redacts_launch_args_and_emails() {
        let s = "launching Game.exe -AUTH_PASSWORD=deadbeef -epicusername=Player for player@example.com";
        let r = redact(s);
        assert!(r.contains("-AUTH_PASSWORD=<redacted>"), "{r}");
        assert!(
            r.contains("<email>") && !r.contains("player@example.com"),
            "{r}"
        );
    }

    #[test]
    fn leaves_ordinary_lines_alone() {
        let s = "applied schema migration version=1";
        assert_eq!(redact(s), s);
    }
}
