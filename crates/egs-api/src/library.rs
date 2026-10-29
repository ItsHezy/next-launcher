//! Owned items, installable builds, catalog metadata and playtime.
//! Spec: docs/protocol/library.md.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::ApiError;
use crate::http::{Client, Retry};
use crate::secret::Secret;

/// An installable build the account is entitled to download.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub app_name: String,
    #[serde(default)]
    pub label_name: String,
    #[serde(default)]
    pub build_version: String,
    pub catalog_item_id: String,
    pub namespace: String,
    #[serde(default)]
    pub asset_id: String,
    #[serde(default)]
    pub sidecar_rvn: Option<u64>,
}

/// `GET /launcher/api/public/assets/<platform>?label=Live`.
pub async fn assets(
    client: &Client,
    access: &Secret,
    platform: &str,
) -> Result<Vec<Asset>, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/launcher/api/public/assets/{}",
            client.hosts().launcher,
            platform
        ))
        .query(&[("label", "Live")])
        .bearer_auth(access.expose());
    client.json(req, Retry::Idempotent).await
}

/// One owned item (game, DLC, subscription, …).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRecord {
    pub namespace: String,
    pub catalog_item_id: String,
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub sandbox_name: Option<String>,
    #[serde(default)]
    pub record_type: Option<String>,
    #[serde(default)]
    pub acquisition_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseMetadata {
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    state_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryPage {
    #[serde(default)]
    response_metadata: Option<ResponseMetadata>,
    #[serde(default)]
    records: Vec<LibraryRecord>,
}

/// All owned items (paginates) and the library state token.
pub async fn library_items(
    client: &Client,
    access: &Secret,
) -> Result<(Vec<LibraryRecord>, Option<String>), ApiError> {
    let mut records = Vec::new();
    let mut cursor: Option<String> = None;
    let mut state_token = None;
    // A page cap guards against a server that never stops returning cursors.
    for _ in 0..200 {
        let mut req = client
            .http()
            .get(format!(
                "{}/library/api/public/items",
                client.hosts().library
            ))
            .query(&[("includeMetadata", "true")])
            .bearer_auth(access.expose());
        if let Some(c) = &cursor {
            req = req.query(&[("cursor", c.as_str())]);
        }
        let page: LibraryPage = client.json(req, Retry::Idempotent).await?;
        records.extend(page.records);
        let meta = page.response_metadata;
        if state_token.is_none() {
            state_token = meta.as_ref().and_then(|m| m.state_token.clone());
        }
        match meta.and_then(|m| m.next_cursor).filter(|c| !c.is_empty()) {
            Some(next) => cursor = Some(next),
            None => return Ok((records, state_token)),
        }
    }
    Err(ApiError::Decode("library pagination did not end".into()))
}

/// `true` if the owned-items list hasn't changed since `token` was issued.
pub async fn state_token_valid(
    client: &Client,
    access: &Secret,
    token: &str,
) -> Result<bool, ApiError> {
    #[derive(Deserialize)]
    struct Status {
        #[serde(default)]
        valid: bool,
    }
    let req = client
        .http()
        .get(format!(
            "{}/library/api/public/stateToken/{}/status",
            client.hosts().library,
            token
        ))
        .bearer_auth(access.expose());
    Ok(client.json::<Status>(req, Retry::Idempotent).await?.valid)
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KeyImage {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Category {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub platform: Option<Vec<String>>,
}

/// Catalog metadata. `raw` keeps the full JSON (custom attributes are read in later phases).
#[derive(Debug, Clone)]
pub struct CatalogItem {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub developer: String,
    pub key_images: Vec<KeyImage>,
    pub categories: Vec<Category>,
    pub release_info: Vec<ReleaseInfo>,
    pub main_game_item: Option<String>,
    pub custom_attributes: HashMap<String, String>,
    pub last_modified: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogItemWire {
    id: String,
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    developer: String,
    #[serde(default)]
    key_images: Vec<KeyImage>,
    #[serde(default)]
    categories: Vec<Category>,
    #[serde(default)]
    release_info: Vec<ReleaseInfo>,
    #[serde(default)]
    main_game_item: Option<serde_json::Value>,
    #[serde(default)]
    custom_attributes: HashMap<String, serde_json::Value>,
    #[serde(default)]
    last_modified_date: Option<String>,
}

impl CatalogItem {
    fn from_value(raw: serde_json::Value) -> Result<Self, serde_json::Error> {
        let w: CatalogItemWire = serde_json::from_value(raw.clone())?;
        let custom_attributes = w
            .custom_attributes
            .into_iter()
            .filter_map(|(k, v)| {
                v.get("value")
                    .and_then(|x| x.as_str())
                    .map(|s| (k, s.to_owned()))
            })
            .collect();
        Ok(Self {
            id: w.id,
            namespace: w.namespace,
            title: w.title,
            developer: w.developer,
            key_images: w.key_images,
            categories: w.categories,
            release_info: w.release_info,
            main_game_item: w
                .main_game_item
                .and_then(|m| m.get("id").and_then(|i| i.as_str()).map(str::to_owned)),
            custom_attributes,
            last_modified: w.last_modified_date,
            raw,
        })
    }

    pub fn is_dlc(&self) -> bool {
        self.main_game_item.is_some()
    }

    pub fn has_category(&self, path: &str) -> bool {
        self.categories.iter().any(|c| c.path == path)
    }

    /// The image best suited for a slot, by preference order.
    pub fn image(&self, prefs: &[&str]) -> Option<&KeyImage> {
        prefs
            .iter()
            .find_map(|p| self.key_images.iter().find(|k| k.kind == *p))
    }
}

/// Max catalog ids per bulk request (docs/protocol/library.md; conservative).
pub const CATALOG_BATCH: usize = 25;

/// `GET /catalog/api/shared/namespace/<ns>/bulk/items?id=…` for up to [`CATALOG_BATCH`] ids.
pub async fn catalog_items(
    client: &Client,
    access: &Secret,
    namespace: &str,
    ids: &[&str],
    country: &str,
    locale: &str,
) -> Result<HashMap<String, CatalogItem>, ApiError> {
    let mut query: Vec<(&str, &str)> = ids.iter().map(|id| ("id", *id)).collect();
    query.extend([
        ("includeDLCDetails", "true"),
        ("includeMainGameDetails", "true"),
        ("country", country),
        ("locale", locale),
    ]);
    let req = client
        .http()
        .get(format!(
            "{}/catalog/api/shared/namespace/{}/bulk/items",
            client.hosts().catalog,
            namespace
        ))
        .query(&query)
        .bearer_auth(access.expose());
    let map: HashMap<String, serde_json::Value> = client.json(req, Retry::Idempotent).await?;
    let mut out = HashMap::with_capacity(map.len());
    for (id, raw) in map {
        match CatalogItem::from_value(raw) {
            Ok(item) => {
                out.insert(id, item);
            }
            Err(e) => tracing::warn!(%namespace, error = %e, "skipping unreadable catalog item"),
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Playtime {
    pub artifact_id: String,
    #[serde(default)]
    pub total_time: u64,
}

/// `GET /library/api/public/playtime/account/<id>/all` (seconds per artifact).
pub async fn playtime_all(
    client: &Client,
    access: &Secret,
    account_id: &str,
) -> Result<Vec<Playtime>, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/library/api/public/playtime/account/{}/all",
            client.hosts().library,
            account_id
        ))
        .bearer_auth(access.expose());
    client.json(req, Retry::Idempotent).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn library_items_paginates_and_keeps_state_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .and(query_param("cursor", "c2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "responseMetadata": {},
                "records": [{ "namespace": "ns2", "catalogItemId": "i2", "appName": "B", "recordType": "APPLICATION" }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "responseMetadata": { "nextCursor": "c2", "stateToken": "st-1" },
                "records": [{ "namespace": "ns1", "catalogItemId": "i1", "appName": "A", "recordType": "APPLICATION" }]
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let (records, token) = library_items(&c, &Secret::new("t")).await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].app_name.as_deref(), Some("B"));
        assert_eq!(token.as_deref(), Some("st-1"));
    }

    #[tokio::test]
    async fn catalog_items_parse_attributes_and_images() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/catalog/api/shared/namespace/ns1/bulk/items"))
            .and(query_param("id", "i1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "i1": {
                    "id": "i1", "namespace": "ns1", "title": "Game", "developer": "Studio",
                    "keyImages": [
                        { "type": "DieselGameBoxTall", "url": "https://cdn1.epicgames.com/a.jpg", "md5": "m1" },
                        { "type": "DieselGameBox", "url": "https://cdn1.epicgames.com/b.jpg" }
                    ],
                    "categories": [{ "path": "games" }, { "path": "applications" }],
                    "customAttributes": {
                        "CanRunOffline": { "type": "STRING", "value": "true" },
                        "CloudSaveFolder": { "type": "STRING", "value": "{AppData}/Game/Saved" }
                    },
                    "releaseInfo": [{ "appId": "art1", "platform": ["Windows"] }],
                    "lastModifiedDate": "2026-01-01T00:00:00.000Z",
                    "unknownField": 1
                }
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let items = catalog_items(&c, &Secret::new("t"), "ns1", &["i1"], "US", "en-US")
            .await
            .unwrap();
        let g = &items["i1"];
        assert_eq!(g.title, "Game");
        assert!(!g.is_dlc());
        assert!(g.has_category("games"));
        assert_eq!(
            g.custom_attributes.get("CanRunOffline").map(String::as_str),
            Some("true")
        );
        assert_eq!(g.release_info[0].app_id.as_deref(), Some("art1"));
        assert_eq!(
            g.image(&["OfferImageTall", "DieselGameBoxTall"])
                .map(|k| k.url.as_str()),
            Some("https://cdn1.epicgames.com/a.jpg")
        );
        assert_eq!(g.raw["unknownField"], 1);
    }

    #[tokio::test]
    async fn assets_parse() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/launcher/api/public/assets/Windows"))
            .and(query_param("label", "Live"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "appName": "A", "labelName": "Live-Windows", "buildVersion": "1.0", "catalogItemId": "i1",
                  "namespace": "ns1", "assetId": "A", "metadata": {"installationPoolId": "p"}, "sidecarRvn": 0 }
            ])))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let a = assets(&c, &Secret::new("t"), "Windows").await.unwrap();
        assert_eq!(a[0].build_version, "1.0");
    }
}
