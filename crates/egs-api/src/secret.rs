use std::fmt;

use serde::{Deserialize, Serialize};

/// A token, code or other secret. `Debug`/`Display` never print the value, so a
/// stray `{:?}` in a log line can't leak it. Use [`Secret::expose`] deliberately.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn never_prints_value() {
        let s = Secret::new("eg1~abc");
        assert_eq!(format!("{s:?} {s}"), "Secret(<redacted>) <redacted>");
        assert_eq!(s.expose(), "eg1~abc");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"eg1~abc\"");
    }
}
