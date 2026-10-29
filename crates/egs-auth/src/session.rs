//! The signed-in session: sign in, resume, silent refresh, exchange codes, sign out.
//! Spec: docs/protocol/auth.md §2–§7.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use egs_api::oauth::{self, Grant, TokenResponse};
use egs_api::{ApiError, Client, Secret};
use tokio::sync::Mutex;

use crate::login::Callback;
use crate::store::{StoreError, StoredSession, TokenStore};

/// Refresh the access token when less than this remains.
const REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("not signed in")]
    NotSignedIn,
    #[error("your Epic session has ended; please sign in again")]
    SessionExpired,
    #[error("Epic needs you to complete a step on its website ({action})")]
    CorrectiveAction { action: String, url: String },
    #[error("sign-in didn't finish: {0}")]
    LoginFailed(String),
    #[error(transparent)]
    Api(ApiError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<ApiError> for AuthError {
    fn from(e: ApiError) -> Self {
        if let Some((action, url)) = e.corrective_action() {
            return Self::CorrectiveAction {
                action: action.to_owned(),
                url: url.to_owned(),
            };
        }
        if e.is_auth_invalid() {
            return Self::SessionExpired;
        }
        Self::Api(e)
    }
}

impl AuthError {
    /// Network trouble or a server hiccup: keep the stored session and try later.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Api(e) if e.is_transient())
    }
}

struct Live {
    account_id: String,
    display_name: String,
    access: Secret,
    access_expires: Instant,
    refresh: Secret,
    refresh_expires_at: u64,
}

pub struct Session {
    api: Client,
    store: Arc<dyn TokenStore>,
    live: Mutex<Option<Live>>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Session {
    pub fn new(api: Client, store: Arc<dyn TokenStore>) -> Self {
        Self {
            api,
            store,
            live: Mutex::new(None),
        }
    }

    pub fn api(&self) -> &Client {
        &self.api
    }

    /// Turn a token response into the live session and persist the refresh token.
    /// The new refresh token is saved before the old one is dropped (rotation).
    fn adopt(&self, t: TokenResponse, prev: Option<&Live>) -> Result<Live, AuthError> {
        let account_id = t
            .account_id
            .or_else(|| prev.map(|p| p.account_id.clone()))
            .ok_or_else(|| AuthError::LoginFailed("Epic didn't return an account id".into()))?;
        let display_name = t
            .display_name
            .or_else(|| prev.map(|p| p.display_name.clone()))
            .unwrap_or_default();
        let (refresh, refresh_expires_at) = match (t.refresh_token, prev) {
            (Some(r), _) => (r, t.refresh_expires.map(|s| now_unix() + s).unwrap_or(0)),
            (None, Some(p)) => (p.refresh.clone(), p.refresh_expires_at),
            (None, None) => {
                return Err(AuthError::LoginFailed(
                    "Epic didn't return a refresh token".into(),
                ))
            }
        };
        let stored = StoredSession {
            account_id: account_id.clone(),
            refresh_token: refresh.clone(),
            refresh_expires_at,
        };
        if let Err(e) = self.store.save(&stored) {
            // This session still works; the next start will ask to sign in again.
            tracing::error!(error = %e, "could not save the session to the credential store");
        }
        Ok(Live {
            account_id,
            display_name,
            access: t.access_token,
            access_expires: Instant::now() + Duration::from_secs(t.expires_in),
            refresh,
            refresh_expires_at,
        })
    }

    fn summary(l: &Live) -> AccountSummary {
        AccountSummary {
            account_id: l.account_id.clone(),
            display_name: l.display_name.clone(),
        }
    }

    /// Finish a login-window sign-in.
    pub async fn sign_in(
        &self,
        callback: Callback,
        verifier: Option<Secret>,
    ) -> Result<AccountSummary, AuthError> {
        let grant = match callback {
            Callback::AuthorizationCode(code) => Grant::AuthorizationCode {
                code,
                code_verifier: verifier,
            },
            Callback::ExchangeCode(code) => Grant::ExchangeCode(code),
            Callback::Failed(why) => return Err(AuthError::LoginFailed(why)),
        };
        let t = oauth::token(&self.api, &grant).await?;
        let mut live = self.live.lock().await;
        let l = self.adopt(t, None)?;
        let s = Self::summary(&l);
        tracing::info!("signed in");
        *live = Some(l);
        Ok(s)
    }

    /// Restore the session saved for `account_id` (silent refresh on start).
    pub async fn resume(&self, account_id: &str) -> Result<AccountSummary, AuthError> {
        let Some(stored) = self.store.load(account_id)? else {
            return Err(AuthError::NotSignedIn);
        };
        if stored.refresh_expires_at != 0 && stored.refresh_expires_at <= now_unix() {
            self.store.delete(account_id)?;
            return Err(AuthError::SessionExpired);
        }
        let mut live = self.live.lock().await;
        let result = oauth::token(
            &self.api,
            &Grant::RefreshToken(stored.refresh_token.clone()),
        )
        .await;
        let t = match result {
            Ok(t) => t,
            Err(e) => {
                let err = AuthError::from(e);
                if matches!(err, AuthError::SessionExpired) {
                    self.store.delete(account_id)?;
                }
                return Err(err);
            }
        };
        let prev = Live {
            account_id: stored.account_id,
            display_name: String::new(),
            access: Secret::new(""),
            access_expires: Instant::now(),
            refresh: stored.refresh_token,
            refresh_expires_at: stored.refresh_expires_at,
        };
        let l = self.adopt(t, Some(&prev))?;
        let s = Self::summary(&l);
        *live = Some(l);
        Ok(s)
    }

    /// A valid access token, refreshing first if it expires within 5 minutes.
    /// Only one refresh runs at a time (the lock is held across it).
    pub async fn access_token(&self) -> Result<Secret, AuthError> {
        let mut live = self.live.lock().await;
        let Some(cur) = live.as_ref() else {
            return Err(AuthError::NotSignedIn);
        };
        if cur.access_expires.saturating_duration_since(Instant::now()) > REFRESH_MARGIN {
            return Ok(cur.access.clone());
        }
        match oauth::token(&self.api, &Grant::RefreshToken(cur.refresh.clone())).await {
            Ok(t) => {
                let l = self.adopt(t, Some(cur))?;
                let access = l.access.clone();
                *live = Some(l);
                Ok(access)
            }
            Err(e) => {
                let err = AuthError::from(e);
                if matches!(err, AuthError::SessionExpired) {
                    let id = cur.account_id.clone();
                    *live = None;
                    self.store.delete(&id)?;
                }
                Err(err)
            }
        }
    }

    /// How long until the access token needs refreshing (for a proactive timer).
    pub async fn refresh_due_in(&self) -> Option<Duration> {
        let live = self.live.lock().await;
        live.as_ref().map(|l| {
            l.access_expires
                .saturating_duration_since(Instant::now())
                .saturating_sub(REFRESH_MARGIN)
        })
    }

    pub async fn current(&self) -> Option<AccountSummary> {
        self.live.lock().await.as_ref().map(Self::summary)
    }

    /// Single-use, 5-minute code for a game launch or a web sign-in.
    pub async fn exchange_code(&self) -> Result<Secret, AuthError> {
        let access = self.access_token().await?;
        Ok(oauth::exchange_code(&self.api, &access).await?.code)
    }

    /// End the Epic session (best effort) and forget the stored token.
    pub async fn sign_out(&self) -> Result<(), AuthError> {
        let taken = self.live.lock().await.take();
        if let Some(l) = taken {
            if let Err(e) = oauth::kill_session(&self.api, &l.access).await {
                tracing::warn!(error = %e, "could not end the Epic session; continuing sign-out");
            }
            self.store.delete(&l.account_id)?;
            tracing::info!("signed out");
        }
        Ok(())
    }

    /// Forget a stored session without network (e.g. when no live session exists).
    pub fn forget(&self, account_id: &str) -> Result<(), AuthError> {
        Ok(self.store.delete(account_id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use egs_api::{ClientConfig, Hosts};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(base: &str) -> Client {
        let mut cfg = ClientConfig::new("test");
        cfg.hosts = Hosts::all(base);
        Client::new(cfg).unwrap()
    }

    fn token_body(access: &str, refresh: &str, expires_in: u64) -> serde_json::Value {
        serde_json::json!({
            "access_token": access, "expires_in": expires_in, "token_type": "bearer",
            "refresh_token": refresh, "refresh_expires": 1987200,
            "account_id": "acc1", "displayName": "Player"
        })
    }

    #[tokio::test]
    async fn sign_in_persists_and_resume_rotates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/api/oauth/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(token_body("eg1~a1", "eg1~r1", 28800)),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/account/api/oauth/token"))
            // Form encoding turns `~` into `%7E`.
            .and(body_string_contains("refresh_token=eg1%7Er1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(token_body("eg1~a2", "eg1~r2", 28800)),
            )
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::default());
        let s = Session::new(client(&server.uri()), store.clone());
        let who = s
            .sign_in(
                Callback::AuthorizationCode(Secret::new("code")),
                Some(Secret::new("v")),
            )
            .await
            .unwrap();
        assert_eq!(who.display_name, "Player");
        assert_eq!(
            store.load("acc1").unwrap().unwrap().refresh_token.expose(),
            "eg1~r1"
        );

        // A fresh process resumes from the store; the refresh token rotates.
        let s2 = Session::new(client(&server.uri()), store.clone());
        s2.resume("acc1").await.unwrap();
        assert_eq!(
            store.load("acc1").unwrap().unwrap().refresh_token.expose(),
            "eg1~r2"
        );
        assert_eq!(s2.access_token().await.unwrap().expose(), "eg1~a2");
    }

    #[tokio::test]
    async fn invalid_refresh_clears_store() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.account.auth_token.invalid_refresh_token"
            })))
            .mount(&server)
            .await;
        let store = Arc::new(MemoryStore::default());
        store
            .save(&StoredSession {
                account_id: "acc1".into(),
                refresh_token: Secret::new("old"),
                refresh_expires_at: 0,
            })
            .unwrap();
        let s = Session::new(client(&server.uri()), store.clone());
        assert!(matches!(
            s.resume("acc1").await,
            Err(AuthError::SessionExpired)
        ));
        assert!(store.load("acc1").unwrap().is_none());
    }

    #[tokio::test]
    async fn network_failure_keeps_store() {
        // Nothing listens on this port: connection refused (transient).
        let store = Arc::new(MemoryStore::default());
        store
            .save(&StoredSession {
                account_id: "acc1".into(),
                refresh_token: Secret::new("keep"),
                refresh_expires_at: 0,
            })
            .unwrap();
        let s = Session::new(client("http://127.0.0.1:9"), store.clone());
        let err = s.resume("acc1").await.unwrap_err();
        assert!(err.is_transient(), "{err:?}");
        assert!(store.load("acc1").unwrap().is_some());
    }

    #[tokio::test]
    async fn access_token_refreshes_near_expiry_only_once() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("grant_type=authorization_code"))
            // Expires in 60 s: inside the 5-minute margin.
            .respond_with(ResponseTemplate::new(200).set_body_json(token_body(
                "eg1~short",
                "eg1~r1",
                60,
            )))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(body_string_contains("grant_type=refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_body(
                "eg1~fresh",
                "eg1~r2",
                28800,
            )))
            .expect(1)
            .mount(&server)
            .await;
        let s = Arc::new(Session::new(
            client(&server.uri()),
            Arc::new(MemoryStore::default()),
        ));
        s.sign_in(Callback::AuthorizationCode(Secret::new("c")), None)
            .await
            .unwrap();
        // Concurrent callers share one refresh.
        let (a, b) = tokio::join!(s.access_token(), s.access_token());
        assert_eq!(a.unwrap().expose(), "eg1~fresh");
        assert_eq!(b.unwrap().expose(), "eg1~fresh");
    }
}
