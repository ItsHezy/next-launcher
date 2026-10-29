//! OAuth endpoints on the account service. Spec: docs/protocol/auth.md.

use serde::Deserialize;

use crate::error::ApiError;
use crate::http::{Client, Retry};
use crate::secret::Secret;

/// EGL's own public OAuth client (ADR 0002). The secret ships inside EGL and is
/// published by Legendary and community documentation; it is not a user secret.
pub const LAUNCHER_CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const LAUNCHER_CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";

/// How to obtain tokens.
#[derive(Debug, Clone)]
pub enum Grant {
    /// Code from the login page's `/id/api/redirect`, with the PKCE verifier if one was used.
    AuthorizationCode {
        code: Secret,
        code_verifier: Option<Secret>,
    },
    RefreshToken(Secret),
    ExchangeCode(Secret),
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: Secret,
    /// Seconds. Prefer this over `expires_at` (avoids clock skew).
    pub expires_in: u64,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<Secret>,
    #[serde(default)]
    pub refresh_expires: Option<u64>,
    #[serde(default)]
    pub refresh_expires_at: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
}

/// `POST /account/api/oauth/token`. Never retried automatically: refresh tokens rotate.
pub async fn token(client: &Client, grant: &Grant) -> Result<TokenResponse, ApiError> {
    let mut form: Vec<(&str, &str)> = vec![("token_type", "eg1")];
    match grant {
        Grant::AuthorizationCode {
            code,
            code_verifier,
        } => {
            form.push(("grant_type", "authorization_code"));
            form.push(("code", code.expose()));
            if let Some(v) = code_verifier {
                form.push(("code_verifier", v.expose()));
            }
        }
        Grant::RefreshToken(t) => {
            form.push(("grant_type", "refresh_token"));
            form.push(("refresh_token", t.expose()));
        }
        Grant::ExchangeCode(c) => {
            form.push(("grant_type", "exchange_code"));
            form.push(("exchange_code", c.expose()));
        }
    }
    let req = client
        .http()
        .post(format!(
            "{}/account/api/oauth/token",
            client.hosts().account
        ))
        .basic_auth(LAUNCHER_CLIENT_ID, Some(LAUNCHER_CLIENT_SECRET))
        .form(&form);
    client.json(req, Retry::Never).await
}

#[derive(Debug, Clone, Deserialize)]
pub struct Permission {
    pub resource: String,
    /// Bitmask: 1 create, 2 read, 4 update, 8 delete.
    pub action: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifyResponse {
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub perms: Vec<Permission>,
}

/// Permission action bits.
pub const CREATE: u32 = 1;
pub const READ: u32 = 2;
pub const UPDATE: u32 = 4;
pub const DELETE: u32 = 8;

impl VerifyResponse {
    /// Does the token allow `action` on `resource`? A `*` segment in a granted resource
    /// matches any one segment; a trailing `*` matches the rest.
    pub fn allows(&self, resource: &str, action: u32) -> bool {
        self.perms
            .iter()
            .any(|p| p.action & action == action && resource_matches(&p.resource, resource))
    }
}

fn resource_matches(granted: &str, wanted: &str) -> bool {
    let g: Vec<&str> = granted.split(':').collect();
    let w: Vec<&str> = wanted.split(':').collect();
    for (i, seg) in g.iter().enumerate() {
        if *seg == "*" && i == g.len() - 1 {
            return w.len() >= g.len();
        }
        match w.get(i) {
            Some(x) if *seg == "*" || seg == x => {}
            _ => return false,
        }
    }
    g.len() == w.len()
}

/// `GET /account/api/oauth/verify`.
pub async fn verify(
    client: &Client,
    access: &Secret,
    include_perms: bool,
) -> Result<VerifyResponse, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/account/api/oauth/verify",
            client.hosts().account
        ))
        .query(&[("includePerms", if include_perms { "true" } else { "false" })])
        .bearer_auth(access.expose());
    client.json(req, Retry::Idempotent).await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeCode {
    pub code: Secret,
    #[serde(default)]
    pub expires_in_seconds: Option<u64>,
}

/// `GET /account/api/oauth/exchange`: a single-use, 5-minute code (game launch, web sign-in).
pub async fn exchange_code(client: &Client, access: &Secret) -> Result<ExchangeCode, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/account/api/oauth/exchange",
            client.hosts().account
        ))
        .bearer_auth(access.expose());
    // Not retried: each call mints a new code.
    client.json(req, Retry::Never).await
}

/// `DELETE /account/api/oauth/sessions/kill/<access>`: end this session.
pub async fn kill_session(client: &Client, access: &Secret) -> Result<(), ApiError> {
    let req = client
        .http()
        .delete(format!(
            "{}/account/api/oauth/sessions/kill/{}",
            client.hosts().account,
            access.expose()
        ))
        .bearer_auth(access.expose());
    client.empty(req, Retry::Idempotent).await.map(|_| ())
}

/// `DELETE /account/api/oauth/sessions/kill?killType=OTHERS`: sign out everywhere else.
pub async fn kill_other_sessions(client: &Client, access: &Secret) -> Result<(), ApiError> {
    let req = client
        .http()
        .delete(format!(
            "{}/account/api/oauth/sessions/kill",
            client.hosts().account
        ))
        .query(&[("killType", "OTHERS")])
        .bearer_auth(access.expose());
    client.empty(req, Retry::Idempotent).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_matching() {
        let v = VerifyResponse {
            account_id: None,
            display_name: None,
            expires_in: None,
            perms: vec![
                Permission {
                    resource: "account:public:account".into(),
                    action: 6,
                },
                Permission {
                    resource: "fulfillment:public:code".into(),
                    action: 1,
                },
                Permission {
                    resource: "friends:abc:*".into(),
                    action: 15,
                },
                Permission {
                    resource: "presence:_:abc".into(),
                    action: 4,
                },
            ],
        };
        assert!(v.allows("account:public:account", UPDATE));
        assert!(!v.allows("account:public:account", DELETE));
        assert!(v.allows("fulfillment:public:code", CREATE));
        assert!(
            !v.allows("fulfillment:public:code:x", CREATE),
            "no implicit children"
        );
        assert!(v.allows("friends:abc:blocklist", READ));
        assert!(!v.allows("friends:other:blocklist", READ));
        assert!(v.allows("presence:_:abc", UPDATE));
        assert!(!v.allows("presence:_:abc", READ));
    }
    use crate::http::test_client;
    use wiremock::matchers::{body_string_contains, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn authorization_code_grant_parses_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/api/oauth/token"))
            .and(header_exists("authorization"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code_verifier=v123"))
            .and(body_string_contains("token_type=eg1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "eg1~aaa", "expires_in": 28800, "expires_at": "2026-10-29T08:00:00.000Z",
                "token_type": "bearer", "refresh_token": "eg1~rrr", "refresh_expires": 1987200,
                "refresh_expires_at": "2026-11-20T12:00:00.000Z", "account_id": "acc1",
                "client_id": LAUNCHER_CLIENT_ID, "displayName": "Player"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let t = token(
            &c,
            &Grant::AuthorizationCode {
                code: Secret::new("c0de"),
                code_verifier: Some(Secret::new("v123")),
            },
        )
        .await
        .unwrap();
        assert_eq!(t.access_token.expose(), "eg1~aaa");
        assert_eq!(
            t.refresh_token.as_ref().map(Secret::expose),
            Some("eg1~rrr")
        );
        assert_eq!(t.account_id.as_deref(), Some("acc1"));
        assert_eq!(t.display_name.as_deref(), Some("Player"));
        assert_eq!(t.expires_in, 28800);
    }

    #[tokio::test]
    async fn corrective_action_is_exposed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.oauth.corrective_action_required",
                "errorMessage": "Corrective action is required to continue.",
                "correctiveAction": "PRIVACY_POLICY_ACCEPTANCE",
                "continuationUrl": "https://www.epicgames.com/id/continue?x=1"
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let err = token(&c, &Grant::RefreshToken(Secret::new("r")))
            .await
            .unwrap_err();
        assert_eq!(
            err.corrective_action(),
            Some((
                "PRIVACY_POLICY_ACCEPTANCE",
                "https://www.epicgames.com/id/continue?x=1"
            ))
        );
    }

    #[tokio::test]
    async fn invalid_refresh_is_auth_invalid() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.account.auth_token.invalid_refresh_token",
                "errorMessage": "Sorry the refresh token is invalid"
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let err = token(&c, &Grant::RefreshToken(Secret::new("r")))
            .await
            .unwrap_err();
        assert!(err.is_auth_invalid());
    }
}
