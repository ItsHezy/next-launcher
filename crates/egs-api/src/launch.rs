//! Launch-time calls: ownership tokens and playtime reports (docs/protocol/launch.md,
//! library.md "Playtime"). Exchange codes for `-AUTH_PASSWORD` live in `oauth`.

use serde::{Deserialize, Serialize};

use crate::http::{Client, Retry};
use crate::{ApiError, Secret};

/// Epic's ownership token for one owned item: opaque bytes, written to a `.ovt` file
/// for `-epicovt=`. Epic issues it only for the signed-in account's own entitlement.
pub async fn ownership_token(
    client: &Client,
    access: &Secret,
    account_id: &str,
    namespace: &str,
    catalog_item_id: &str,
) -> Result<Vec<u8>, ApiError> {
    let req = client
        .http()
        .post(format!(
            "{}/ecommerceintegration/api/public/platforms/EPIC/identities/{}/ownershipToken",
            client.hosts().ecom,
            account_id
        ))
        .bearer_auth(access.expose())
        .form(&[("nsCatalogItemId", format!("{namespace}:{catalog_item_id}"))]);
    Ok(client
        .send(req, Retry::Never)
        .await?
        .bytes()
        .await?
        .to_vec())
}

/// One play session, reported when the game exits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaySession {
    pub machine_id: String,
    /// `releaseInfo[].appId` from the catalog.
    pub artifact_id: String,
    pub start_time: String,
    pub end_time: String,
    pub start_segment: bool,
    pub end_segment: bool,
}

impl PlaySession {
    /// A whole session, from Unix milliseconds.
    pub fn whole(machine_id: &str, artifact_id: &str, start_ms: i64, end_ms: i64) -> Self {
        Self {
            machine_id: machine_id.to_owned(),
            artifact_id: artifact_id.to_owned(),
            start_time: iso8601(start_ms),
            end_time: iso8601(end_ms),
            start_segment: true,
            end_segment: true,
        }
    }
}

/// Add a session to the account's playtime (204). Not retried here: the caller keeps
/// failed reports and sends them again later, so a session is never counted twice.
pub async fn report_playtime(
    client: &Client,
    access: &Secret,
    account_id: &str,
    session: &PlaySession,
) -> Result<(), ApiError> {
    let req = client
        .http()
        .put(format!(
            "{}/library/api/public/playtime/account/{}",
            client.hosts().library,
            account_id
        ))
        .bearer_auth(access.expose())
        .json(session);
    client.empty(req, Retry::Never).await?;
    Ok(())
}

/// `2026-10-29T14:46:15.794Z` from Unix milliseconds (UTC).
pub fn iso8601(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let ms = unix_ms.rem_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{body_json, body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn iso8601_formats_utc() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1_793_285_175_794), "2026-10-29T14:46:15.794Z");
        assert_eq!(iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[tokio::test]
    async fn ownership_token_posts_the_item_and_returns_raw_bytes() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(
                "/ecommerceintegration/api/public/platforms/EPIC/identities/acc/ownershipToken",
            ))
            .and(header("authorization", "Bearer tok"))
            .and(body_string("nsCatalogItemId=ns%3Aitem"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2, 3, 0, 255]))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let bytes = ownership_token(&c, &Secret::new("tok"), "acc", "ns", "item")
            .await
            .unwrap();
        assert_eq!(bytes, vec![1u8, 2, 3, 0, 255]);
    }

    #[tokio::test]
    async fn ownership_token_refusal_is_an_error_and_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        assert!(
            ownership_token(&c, &Secret::new("tok"), "acc", "ns", "item")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn playtime_report_sends_the_session() {
        let server = MockServer::start().await;
        let s = PlaySession::whole("m-1", "art", 0, 61_000);
        Mock::given(method("PUT"))
            .and(path("/library/api/public/playtime/account/acc"))
            .and(body_json(serde_json::json!({
                "machineId": "m-1", "artifactId": "art",
                "startTime": "1970-01-01T00:00:00.000Z", "endTime": "1970-01-01T00:01:01.000Z",
                "startSegment": true, "endSegment": true
            })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        report_playtime(&c, &Secret::new("tok"), "acc", &s)
            .await
            .unwrap();
    }
}
