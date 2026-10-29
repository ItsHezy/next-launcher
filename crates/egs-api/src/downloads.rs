//! Build lookup, manifest download, download tickets and signed chunk URLs.
//! Spec: docs/protocol/downloads-cdn.md.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::error::ApiError;
use crate::http::{Client, Retry};
use crate::secret::Secret;

#[derive(Debug, Clone, Deserialize)]
struct QueryParam {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestRef {
    uri: String,
    #[serde(default)]
    query_params: Vec<QueryParam>,
}

impl ManifestRef {
    fn url(&self) -> String {
        if self.query_params.is_empty() {
            return self.uri.clone();
        }
        let q: Vec<String> = self
            .query_params
            .iter()
            .map(|p| format!("{}={}", p.name, p.value))
            .collect();
        format!("{}?{}", self.uri, q.join("&"))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildElement {
    app_name: String,
    #[serde(default)]
    label_name: String,
    build_version: String,
    hash: String,
    #[serde(default)]
    use_signed_url: bool,
    #[serde(default)]
    is_preloaded: bool,
    manifests: Vec<ManifestRef>,
    #[serde(default)]
    selective_download: Vec<ManifestRef>,
    #[serde(default)]
    secrets: HashMap<String, String>,
    #[serde(default)]
    sidecar: Option<Sidecar>,
}

#[derive(Debug, Clone, Deserialize)]
struct Sidecar {
    #[serde(default)]
    config: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BuildResponse {
    elements: Vec<BuildElement>,
}

/// Everything needed to download one build.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub app_name: String,
    pub label_name: String,
    pub build_version: String,
    /// SHA-1 (hex) of the manifest file.
    pub manifest_hash: String,
    pub manifest_urls: Vec<String>,
    /// CDN base URLs (`…/CloudDir`), de-duplicated, in Epic's order.
    pub base_urls: Vec<String>,
    pub use_signed_url: bool,
    pub is_preloaded: bool,
    /// Selective-download metadata URL (optional components), if any.
    pub sdmeta_url: Option<String>,
    /// AES keys for encrypted manifests/chunks, from Epic for this entitlement. Never logged.
    pub secrets: HashMap<String, String>,
    /// EOS deployment id from the sidecar config (launch argument).
    pub deployment_id: Option<String>,
}

/// `GET /launcher/api/public/assets/v2/platform/Windows/namespace/<ns>/catalogItem/<id>/app/<app>/label/Live`.
/// 403 here means the account isn't entitled to download this app.
pub async fn build_info(
    client: &Client,
    access: &Secret,
    namespace: &str,
    catalog_item_id: &str,
    app_name: &str,
) -> Result<BuildInfo, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/launcher/api/public/assets/v2/platform/Windows/namespace/{}/catalogItem/{}/app/{}/label/Live",
            client.hosts().launcher,
            namespace,
            catalog_item_id,
            app_name
        ))
        .bearer_auth(access.expose());
    let resp: BuildResponse = client.json(req, Retry::Idempotent).await?;
    let el = match resp.elements.len() {
        1 => resp.elements.into_iter().next(),
        _ => None,
    }
    .ok_or_else(|| ApiError::Decode("expected exactly one build element".into()))?;

    let mut base_urls: Vec<String> = Vec::new();
    for m in &el.manifests {
        if let Some((base, _)) = m.uri.rsplit_once('/') {
            if !base_urls.iter().any(|b| b == base) {
                base_urls.push(base.to_owned());
            }
        }
    }
    let deployment_id = el
        .sidecar
        .and_then(|s| s.config)
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| {
            v.get("deploymentId")
                .and_then(|d| d.as_str())
                .map(str::to_owned)
        });
    Ok(BuildInfo {
        app_name: el.app_name,
        label_name: el.label_name,
        build_version: el.build_version,
        manifest_hash: el.hash.to_lowercase(),
        manifest_urls: el.manifests.iter().map(ManifestRef::url).collect(),
        base_urls,
        use_signed_url: el.use_signed_url,
        is_preloaded: el.is_preloaded,
        sdmeta_url: el.selective_download.first().map(ManifestRef::url),
        secrets: el.secrets,
        deployment_id,
    })
}

/// Download the manifest from the first CDN that answers with the right SHA-1.
/// Manifest URLs are pre-signed (`cf_token`), so no bearer token is sent.
pub async fn fetch_manifest(client: &Client, build: &BuildInfo) -> Result<Vec<u8>, ApiError> {
    let mut last = ApiError::Decode("no manifest URLs".into());
    for url in &build.manifest_urls {
        match client.send(client.http().get(url), Retry::Idempotent).await {
            Ok(resp) => {
                let bytes = resp.bytes().await?;
                let got = Sha1::digest(&bytes);
                let hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
                if hex == build.manifest_hash {
                    return Ok(bytes.to_vec());
                }
                tracing::warn!("manifest hash mismatch from one CDN; trying the next");
                last = ApiError::Decode("manifest hash mismatch".into());
            }
            Err(e) => {
                tracing::debug!(error = %e, "manifest download failed on one CDN");
                last = e;
            }
        }
    }
    Err(last)
}

/// Selective-download metadata (`SelDlMeta`); may be UTF-16LE with BOM.
pub async fn fetch_sdmeta(client: &Client, url: &str) -> Result<SdMeta, ApiError> {
    let bytes = client
        .send(client.http().get(url), Retry::Idempotent)
        .await?
        .bytes()
        .await?;
    parse_sdmeta(&bytes)
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SdComponent {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_required: String,
    #[serde(default)]
    pub unique_id: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl SdComponent {
    pub fn required(&self) -> bool {
        self.is_required.eq_ignore_ascii_case("true")
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SdMeta {
    #[serde(default)]
    pub data: Vec<SdComponent>,
}

/// Parse `.sdmeta` bytes (UTF-8, or UTF-16LE with a BOM as EGL stores them).
pub fn parse_sdmeta(bytes: &[u8]) -> Result<SdMeta, ApiError> {
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes))
            .into_owned()
    };
    serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTicket {
    pub signed_ticket: Secret,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// `POST …/assets/v2/ticket`: needed when the build uses signed chunk URLs.
pub async fn download_ticket(
    client: &Client,
    access: &Secret,
    namespace: &str,
    catalog_item_id: &str,
    app_name: &str,
    build_version: &str,
    label_name: &str,
) -> Result<DownloadTicket, ApiError> {
    let (label, platform) = label_name.split_once('-').unwrap_or(("Live", "Windows"));
    let req = client
        .http()
        .post("https://launcher-public-service-prod.ak.epicgames.com/launcher/api/public/assets/v2/ticket")
        .bearer_auth(access.expose())
        .json(&serde_json::json!({
            "catalogItemId": catalog_item_id,
            "buildVersion": build_version,
            "appName": app_name,
            "namespace": namespace,
            "label": label,
            "platform": platform,
        }));
    client.json(req, Retry::Never).await
}

/// `POST …/delivery/account/<id>/downloadurls`: signed URLs for up to 50 chunk paths.
pub async fn signed_chunk_urls(
    client: &Client,
    access: &Secret,
    account_id: &str,
    ticket: &Secret,
    chunk_paths: &[String],
) -> Result<HashMap<String, String>, ApiError> {
    let req = client
        .http()
        .post(format!(
            "https://artifact-delivery-service-public-prod.ol.epicgames.com/artifact-delivery/api/public/v1/delivery/account/{account_id}/downloadurls"
        ))
        .bearer_auth(access.expose())
        .json(&serde_json::json!({
            "signedTicket": ticket.expose(),
            "chunkIds": chunk_paths,
            "deltaId": "",
            "clientMetrics": {},
        }));
    client.json(req, Retry::Idempotent).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn build_info_and_manifest_hash_check() {
        let server = MockServer::start().await;
        let manifest = b"manifest-bytes".to_vec();
        let hash: String = Sha1::digest(&manifest)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let base = server.uri();
        Mock::given(method("GET"))
            .and(path("/launcher/api/public/assets/v2/platform/Windows/namespace/ns/catalogItem/id/app/App/label/Live"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "elements": [{
                    "appName": "App", "labelName": "Live-Windows", "buildVersion": "1.0", "hash": hash.to_uppercase(),
                    "useSignedUrl": false,
                    "manifests": [
                        { "uri": format!("{base}/bad/CloudDir/x.manifest") },
                        { "uri": format!("{base}/good/CloudDir/x.manifest"), "queryParams": [{ "name": "cf_token", "value": "abc" }] }
                    ],
                    "secrets": { "0000000100000002000000030000000A": "00ff" },
                    "sidecar": { "config": "{\"deploymentId\":\"dep1\"}", "rvn": 1 }
                }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/bad/CloudDir/x.manifest"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"wrong".to_vec()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/good/CloudDir/x.manifest"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(manifest.clone()))
            .mount(&server)
            .await;

        let c = test_client(&base);
        let b = build_info(&c, &Secret::new("t"), "ns", "id", "App")
            .await
            .unwrap();
        assert_eq!(b.base_urls.len(), 2);
        assert!(b.manifest_urls[1].ends_with("?cf_token=abc"));
        assert_eq!(b.deployment_id.as_deref(), Some("dep1"));
        assert_eq!(b.secrets.len(), 1);
        assert_eq!(fetch_manifest(&c, &b).await.unwrap(), manifest);
    }

    #[test]
    fn sdmeta_utf16_with_bom() {
        let json = r#"{"DataType":"SelDlMeta","Data":[{"Title":"Core","IsRequired":"true","UniqueId":"u1","Tags":["a","b"]},{"Title":"Voice DE","IsRequired":"false","UniqueId":"u2","Tags":["de"]}]}"#;
        let mut bytes = vec![0xFF, 0xFE];
        for u in json.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let m = parse_sdmeta(&bytes).unwrap();
        assert_eq!(m.data.len(), 2);
        assert!(m.data[0].required() && !m.data[1].required());
        assert_eq!(m.data[0].tags, vec!["a", "b"]);
        assert_eq!(parse_sdmeta(json.as_bytes()).unwrap(), m);
    }
}
