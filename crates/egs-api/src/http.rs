//! One shared HTTP client for every Epic host (docs/protocol/README.md "good API citizen").

use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use reqwest::{RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;

use crate::error::{ApiError, EpicError};

/// Base URLs of the services we talk to. Overridable so tests can use a mock server.
#[derive(Debug, Clone)]
pub struct Hosts {
    pub account: String,
    pub web: String,
    pub launcher: String,
    pub library: String,
    pub catalog: String,
    pub eula: String,
    /// Ownership tokens (launch.md).
    pub ecom: String,
}

impl Hosts {
    pub fn production() -> Self {
        Self {
            account: "https://account-public-service-prod03.ol.epicgames.com".into(),
            web: "https://www.epicgames.com".into(),
            launcher: "https://launcher-public-service-prod06.ol.epicgames.com".into(),
            library: "https://library-service.live.use1a.on.epicgames.com".into(),
            catalog: "https://catalog-public-service-prod06.ol.epicgames.com".into(),
            eula: "https://eulatracking-public-service-prod06.ol.epicgames.com".into(),
            ecom: "https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com".into(),
        }
    }

    /// Every host pointed at one base URL (tests).
    pub fn all(base: &str) -> Self {
        let b = base.trim_end_matches('/').to_owned();
        Self {
            account: b.clone(),
            web: b.clone(),
            launcher: b.clone(),
            library: b.clone(),
            catalog: b.clone(),
            eula: b.clone(),
            ecom: b,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub user_agent: String,
    pub hosts: Hosts,
    pub connect_timeout: Duration,
    pub timeout: Duration,
    /// Attempts after the first, for idempotent requests.
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl ClientConfig {
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            user_agent: user_agent.into(),
            hosts: Hosts::production(),
            connect_timeout: Duration::from_secs(5),
            timeout: Duration::from_secs(20),
            max_retries: 4,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
        }
    }
}

/// Whether a request may be repeated after a transient failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    /// Never repeat (e.g. token grants: refresh tokens rotate).
    Never,
    /// Safe to repeat on connection errors, 408, 429 and 5xx.
    Idempotent,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    hosts: Arc<Hosts>,
    cfg: Arc<ClientConfig>,
}

static CRYPTO: Once = Once::new();

impl Client {
    pub fn new(cfg: ClientConfig) -> Result<Self, ApiError> {
        CRYPTO.call_once(|| {
            // Ignore "already installed": another crate may have installed one first.
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let http = reqwest::Client::builder()
            .user_agent(cfg.user_agent.clone())
            .connect_timeout(cfg.connect_timeout)
            .timeout(cfg.timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .gzip(true)
            .brotli(true)
            .build()
            .map_err(|e| ApiError::Invalid(e.to_string()))?;
        Ok(Self {
            http,
            hosts: Arc::new(cfg.hosts.clone()),
            cfg: Arc::new(cfg),
        })
    }

    pub fn hosts(&self) -> &Hosts {
        &self.hosts
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    fn backoff(&self, attempt: u32, retry_after: Option<u64>) -> Duration {
        let exp = self
            .cfg
            .base_delay
            .saturating_mul(1u32 << attempt.min(16))
            .min(self.cfg.max_delay);
        // Full jitter.
        let jittered = Duration::from_millis(fastrand::u64(0..=exp.as_millis() as u64));
        match retry_after {
            Some(s) => jittered.max(Duration::from_secs(s)).min(self.cfg.max_delay),
            None => jittered,
        }
    }

    /// Send a request; non-2xx becomes [`ApiError::Http`] with Epic's error body parsed.
    pub async fn send(&self, req: RequestBuilder, retry: Retry) -> Result<Response, ApiError> {
        let request = req.build()?;
        let method = request.method().clone();
        let host = request.url().host_str().unwrap_or("").to_owned();
        let path = request.url().path().to_owned();
        let mut attempt = 0u32;

        loop {
            let this = request
                .try_clone()
                .ok_or_else(|| ApiError::Invalid("request body is not repeatable".into()))?;
            let started = Instant::now();
            let result = self.http.execute(this).await;
            let ms = started.elapsed().as_millis();

            let err = match result {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!(%method, %host, %path, status = resp.status().as_u16(), ms, "epic request");
                    return Ok(resp);
                }
                Ok(resp) => {
                    let status = resp.status();
                    let retry_after = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok());
                    let body = resp.bytes().await.unwrap_or_default();
                    let epic = serde_json::from_slice::<EpicError>(&body)
                        .ok()
                        .filter(|e| !e.error_code.is_empty());
                    tracing::debug!(
                        %method, %host, %path, status = status.as_u16(), ms,
                        code = epic.as_ref().map(|e| e.error_code.as_str()).unwrap_or(""),
                        "epic request failed"
                    );
                    ApiError::Http {
                        status: status.as_u16(),
                        epic: epic.map(Box::new),
                        retry_after,
                    }
                }
                Err(e) => {
                    let err = ApiError::from(e);
                    tracing::debug!(%method, %host, %path, ms, error = %err, "epic request error");
                    err
                }
            };

            if retry == Retry::Idempotent && err.is_transient() && attempt < self.cfg.max_retries {
                let retry_after = match &err {
                    ApiError::Http { retry_after, .. } => *retry_after,
                    _ => None,
                };
                let wait = self.backoff(attempt, retry_after);
                tracing::info!(%host, %path, attempt = attempt + 1, wait_ms = wait.as_millis() as u64, "retrying");
                tokio::time::sleep(wait).await;
                attempt += 1;
                continue;
            }
            return Err(err);
        }
    }

    /// Send and decode a JSON body.
    pub async fn json<T: DeserializeOwned>(
        &self,
        req: RequestBuilder,
        retry: Retry,
    ) -> Result<T, ApiError> {
        let resp = self.send(req, retry).await?;
        let bytes = resp.bytes().await?;
        serde_json::from_slice(&bytes).map_err(|e| ApiError::Decode(e.to_string()))
    }

    /// Send, expecting no body we care about (e.g. 204).
    pub async fn empty(&self, req: RequestBuilder, retry: Retry) -> Result<StatusCode, ApiError> {
        Ok(self.send(req, retry).await?.status())
    }
}

#[cfg(test)]
pub(crate) fn test_client(base: &str) -> Client {
    let mut cfg = ClientConfig::new("NextLauncher-test");
    cfg.hosts = Hosts::all(base);
    cfg.base_delay = Duration::from_millis(5);
    cfg.max_delay = Duration::from_millis(20);
    Client::new(cfg).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn retries_idempotent_on_503_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let v: serde_json::Value = c
            .json(
                c.http().get(format!("{}/x", server.uri())),
                Retry::Idempotent,
            )
            .await
            .unwrap();
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn never_retries_non_idempotent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let err = c
            .send(c.http().post(format!("{}/t", server.uri())), Retry::Never)
            .await
            .unwrap_err();
        assert_eq!(err.status(), Some(503));
    }

    #[tokio::test]
    async fn parses_epic_error_envelope() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.account.oauth.single_use_code_not_found",
                "message": "Sorry, the code is not valid.",
                "numericErrorCode": 18001
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let err = c
            .send(
                c.http().get(format!("{}/e", server.uri())),
                Retry::Idempotent,
            )
            .await
            .unwrap_err();
        assert_eq!(
            err.epic_code(),
            Some("errors.com.epicgames.account.oauth.single_use_code_not_found")
        );
        assert!(err.is_auth_invalid());
        assert!(!err.is_transient());
    }
}
