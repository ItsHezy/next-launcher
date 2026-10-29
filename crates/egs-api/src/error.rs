//! Errors. Epic services share one JSON error envelope (docs/protocol/README.md).

use serde::Deserialize;

/// Epic's error body. Some web routes use `message` instead of `errorMessage`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicError {
    #[serde(default)]
    pub error_code: String,
    #[serde(default, alias = "message")]
    pub error_message: String,
    #[serde(default)]
    pub numeric_error_code: Option<i64>,
    /// Present with `errors.com.epicgames.oauth.corrective_action_required`.
    #[serde(default)]
    pub corrective_action: Option<String>,
    #[serde(default)]
    pub continuation_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("couldn't reach Epic's servers: {0}")]
    Network(String),
    #[error("Epic's servers didn't answer in time")]
    Timeout,
    #[error("Epic returned HTTP {status}{}", .epic.as_ref().map(|e| format!(" ({})", e.error_code)).unwrap_or_default())]
    Http {
        status: u16,
        /// Boxed: keeps `Result<_, ApiError>` small (clippy::result_large_err).
        epic: Option<Box<EpicError>>,
        /// Seconds from a `Retry-After` header, if any.
        retry_after: Option<u64>,
    },
    #[error("unexpected response from Epic: {0}")]
    Decode(String),
    #[error("invalid request: {0}")]
    Invalid(String),
}

impl ApiError {
    pub fn epic_code(&self) -> Option<&str> {
        match self {
            Self::Http { epic: Some(e), .. } => Some(e.error_code.as_str()),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// The refresh token (or session) is no longer accepted: the user must sign in again.
    pub fn is_auth_invalid(&self) -> bool {
        matches!(
            self.epic_code(),
            Some(
                "errors.com.epicgames.account.auth_token.invalid_refresh_token"
                    | "errors.com.epicgames.account.auth_token.invalid_access_token"
                    | "errors.com.epicgames.common.oauth.invalid_token"
                    | "errors.com.epicgames.account.oauth.authorization_code_not_found"
                    | "errors.com.epicgames.account.oauth.single_use_code_not_found"
                    | "errors.com.epicgames.account.oauth.exchange_code_not_found"
                    | "errors.com.epicgames.common.authentication.token_verification_failed"
            )
        ) || (self.status() == Some(401) && self.epic_code().is_none())
    }

    /// Epic wants the user to complete something on its website first.
    pub fn corrective_action(&self) -> Option<(&str, &str)> {
        match self {
            Self::Http { epic: Some(e), .. } => match (&e.corrective_action, &e.continuation_url) {
                (Some(a), Some(u)) => Some((a.as_str(), u.as_str())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Worth retrying later (network trouble, rate limit, server error).
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Network(_) | Self::Timeout => true,
            Self::Http { status, .. } => matches!(status, 408 | 429 | 500 | 502 | 503 | 504),
            _ => false,
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout
        } else if e.is_decode() {
            Self::Decode(e.to_string())
        } else if e.is_builder() {
            Self::Invalid(e.to_string())
        } else {
            // Strip the URL: it may carry query parameters.
            Self::Network(e.without_url().to_string())
        }
    }
}
