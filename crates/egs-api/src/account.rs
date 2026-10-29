//! Account service. Spec: docs/protocol/account.md.

use serde::Deserialize;

use crate::error::ApiError;
use crate::http::{Client, Retry};
use crate::secret::Secret;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub preferred_language: Option<String>,
    #[serde(default)]
    pub tfa_enabled: Option<bool>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub can_update_display_name: Option<bool>,
}

/// `GET /account/api/public/account/<id>`.
pub async fn account_info(
    client: &Client,
    access: &Secret,
    account_id: &str,
) -> Result<AccountInfo, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/account/api/public/account/{}",
            client.hosts().account,
            account_id
        ))
        .bearer_auth(access.expose());
    client.json(req, Retry::Idempotent).await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAuth {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub external_display_name: Option<String>,
    #[serde(default)]
    pub date_added: Option<String>,
}

/// `GET /account/api/public/account/<id>/externalAuths` (connected accounts).
pub async fn external_auths(
    client: &Client,
    access: &Secret,
    account_id: &str,
) -> Result<Vec<ExternalAuth>, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/account/api/public/account/{}/externalAuths",
            client.hosts().account,
            account_id
        ))
        .bearer_auth(access.expose());
    client.json(req, Retry::Idempotent).await
}

/// A pending agreement from EULA tracking, or `None` when nothing needs accepting.
#[derive(Debug, Clone, Deserialize)]
pub struct PendingAgreement {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub title: Option<String>,
}

/// `GET /eulatracking/api/public/agreements/<eula>/account/<id>`: 204 = nothing pending.
pub async fn eula_status(
    client: &Client,
    access: &Secret,
    eula_id: &str,
    account_id: &str,
    locale: &str,
) -> Result<Option<PendingAgreement>, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/eulatracking/api/public/agreements/{}/account/{}",
            client.hosts().eula,
            eula_id,
            account_id
        ))
        .query(&[("locale", locale)])
        .bearer_auth(access.expose());
    let resp = client.send(req, Retry::Idempotent).await?;
    if resp.status().as_u16() == 204 {
        return Ok(None);
    }
    let bytes = resp.bytes().await?;
    if bytes.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| ApiError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn eula_204_means_nothing_pending() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/eulatracking/api/public/agreements/epicgames_tos/account/a1",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let r = eula_status(&c, &Secret::new("t"), "epicgames_tos", "a1", "en")
            .await
            .unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn parses_account_info() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/api/public/account/a1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "a1", "displayName": "Player", "email": "p@example.com", "country": "US",
                "preferredLanguage": "en", "tfaEnabled": true, "unknownField": 1
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let a = account_info(&c, &Secret::new("t"), "a1").await.unwrap();
        assert_eq!(a.display_name, "Player");
        assert_eq!(a.tfa_enabled, Some(true));
    }
}
