//! Owned games managed by another store: handed to that store's launcher
//! (docs/protocol/launch.md "Third-party launchers").

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Ea,
    Ubisoft,
}

impl Store {
    /// From catalog `customAttributes.ThirdPartyManagedApp` / `...Provider`.
    pub fn from_attr(value: &str) -> Option<Self> {
        let v = value.to_ascii_lowercase();
        if v.contains("origin") || v.contains("ea app") || v == "ea" {
            Some(Self::Ea)
        } else if v.contains("uplay") || v.contains("ubisoft") {
            Some(Self::Ubisoft)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ea => "EA app",
            Self::Ubisoft => "Ubisoft Connect",
        }
    }
}

pub struct Handoff<'a> {
    pub store: Store,
    /// EA: the Epic `appName`. Ubisoft: `customAttributes.GameID`, else the `appName`.
    pub game_id: &'a str,
    pub exchange_code: &'a str,
    pub display_name: &'a str,
    pub account_id: &'a str,
    pub locale: &'a str,
    /// `customAttributes.AdditionalCommandLine`, as `k=v&k2=v2` pairs.
    pub additional: &'a str,
}

/// The `link2ea://` / `uplay://` URL to open with the shell.
pub fn url(h: &Handoff<'_>) -> String {
    let mut pairs: Vec<(String, String)> = vec![
        ("AUTH_PASSWORD".into(), h.exchange_code.into()),
        ("AUTH_TYPE".into(), "exchangecode".into()),
        ("epicusername".into(), h.display_name.into()),
        ("epicuserid".into(), h.account_id.into()),
        ("epiclocale".into(), h.locale.into()),
    ];
    for kv in h.additional.split('&').filter(|s| !s.trim().is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        pairs.push((k.trim().into(), v.trim().into()));
    }
    let query = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let base = match h.store {
        Store::Ea => "link2ea://launchgame/",
        Store::Ubisoft => "uplay://launch/",
    };
    format!("{base}{}?{query}", encode(h.game_id))
}

/// Percent-encode everything except RFC 3986 unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_names() {
        assert_eq!(Store::from_attr("Origin"), Some(Store::Ea));
        assert_eq!(Store::from_attr("The EA app"), Some(Store::Ea));
        assert_eq!(Store::from_attr("Uplay"), Some(Store::Ubisoft));
        assert_eq!(Store::from_attr("Steam"), None);
    }

    #[test]
    fn builds_handoff_urls() {
        let mut h = Handoff {
            store: Store::Ea,
            game_id: "Origin.OFR.50.0001",
            exchange_code: "c0de",
            display_name: "Player One",
            account_id: "acc",
            locale: "en-US",
            additional: "",
        };
        assert_eq!(
            url(&h),
            "link2ea://launchgame/Origin.OFR.50.0001?AUTH_PASSWORD=c0de&AUTH_TYPE=exchangecode\
             &epicusername=Player%20One&epicuserid=acc&epiclocale=en-US"
        );
        h.store = Store::Ubisoft;
        h.game_id = "635";
        h.additional = "theme=dark&x=1";
        assert!(url(&h).starts_with("uplay://launch/635?AUTH_PASSWORD=c0de&"));
        assert!(url(&h).ends_with("&epiclocale=en-US&theme=dark&x=1"));
    }
}
