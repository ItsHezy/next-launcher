//! Cloud saves storage (docs/protocol/cloud-saves.md). Listing and link requests use our
//! bearer; the pre-signed read/write links are fetched without it.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::ApiError;
use crate::http::{Client, Retry};
use crate::secret::Secret;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveFile {
    #[serde(default)]
    pub read_link: Option<String>,
    #[serde(default)]
    pub write_link: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct SaveFiles {
    /// Full storage path → file. Paths look like `.../<accountId>/<appName>/manifests/<ts>.manifest`.
    #[serde(default)]
    pub files: BTreeMap<String, SaveFile>,
}

impl SaveFiles {
    /// Manifest file names (`2026.10.29-14.05.33.manifest`), oldest first.
    pub fn manifests(&self) -> Vec<(&str, &SaveFile)> {
        let mut out: Vec<(&str, &SaveFile)> = self
            .files
            .iter()
            .filter_map(|(path, f)| {
                let name = path.rsplit('/').next()?;
                (path.contains("/manifests/") && name.ends_with(".manifest")).then_some((name, f))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// The storage prefix for a game (`<...>/<accountId>/<appName>/`), from any listed path.
    pub fn prefix_for(&self, app_name: &str) -> Option<String> {
        let needle = format!("/{app_name}/");
        self.files
            .keys()
            .find_map(|k| k.find(&needle).map(|i| k[..i + needle.len()].to_owned()))
    }
}

fn base(client: &Client, account_id: &str, app_name: &str) -> String {
    format!(
        "{}/api/v1/access/egstore/savesync/{}/{}/",
        client.hosts().datastorage,
        account_id,
        app_name
    )
}

/// Every save file the account has for one game (manifests and chunks), with read links.
pub async fn list(
    client: &Client,
    access: &Secret,
    account_id: &str,
    app_name: &str,
) -> Result<SaveFiles, ApiError> {
    let req = client
        .http()
        .get(base(client, account_id, app_name))
        .bearer_auth(access.expose());
    client.json(req, Retry::Idempotent).await
}

/// Links for paths relative to the game's folder (`manifests/x.manifest`, `ChunksV4/..`):
/// read links for existing files, write links for new ones.
pub async fn links(
    client: &Client,
    access: &Secret,
    account_id: &str,
    app_name: &str,
    paths: &[String],
) -> Result<SaveFiles, ApiError> {
    let req = client
        .http()
        .post(base(client, account_id, app_name))
        .bearer_auth(access.expose())
        .json(&serde_json::json!({ "files": paths }));
    client.json(req, Retry::Idempotent).await
}

/// GET a pre-signed link (no bearer).
pub async fn fetch(client: &Client, url: &str) -> Result<Vec<u8>, ApiError> {
    let resp = client
        .send(client.http().get(url), Retry::Idempotent)
        .await?;
    Ok(resp.bytes().await?.to_vec())
}

/// PUT to a pre-signed link (no bearer). Idempotent: the same bytes to the same path.
pub async fn store(client: &Client, url: &str, body: Vec<u8>) -> Result<(), ApiError> {
    client
        .empty(client.http().put(url).body(body), Retry::Idempotent)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_and_links_use_the_savesync_paths() {
        let server = MockServer::start().await;
        let listing = serde_json::json!({ "files": {
            "egstore/acc/app/manifests/2026.10.29-10.00.00.manifest": { "readLink": "https://r/1", "lastModified": "2026-10-29T10:00:01.000Z", "size": 300 },
            "egstore/acc/app/manifests/2026.10.28-09.00.00.manifest": { "readLink": "https://r/0" },
            "egstore/acc/app/ChunksV4/12/ABC_DEF.chunk": { "readLink": "https://r/2" }
        }});
        Mock::given(method("GET"))
            .and(path("/api/v1/access/egstore/savesync/acc/app/"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(listing))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/access/egstore/savesync/acc/app/"))
            .and(body_json(
                serde_json::json!({ "files": ["manifests/x.manifest"] }),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "files": {
                "egstore/acc/app/manifests/x.manifest": { "writeLink": "https://w/x" } } })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let t = Secret::new("tok");
        let l = list(&c, &t, "acc", "app").await.unwrap();
        let names: Vec<&str> = l.manifests().iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            [
                "2026.10.28-09.00.00.manifest",
                "2026.10.29-10.00.00.manifest"
            ]
        );
        assert_eq!(l.prefix_for("app").as_deref(), Some("egstore/acc/app/"));
        let w = links(&c, &t, "acc", "app", &["manifests/x.manifest".to_owned()])
            .await
            .unwrap();
        assert_eq!(
            w.files
                .values()
                .next()
                .and_then(|f| f.write_link.as_deref()),
            Some("https://w/x")
        );
    }
}
