//! Wallet summary and code redemption (docs/protocol/wallet-and-redeem.md).
//! Balances are display strings formatted by Epic; we never do currency math.

use serde::Deserialize;

use crate::error::ApiError;
use crate::http::{Client, Retry};
use crate::secret::Secret;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalletSummary {
    /// `None` when the wallet isn't offered in the account's region.
    pub balance: Option<String>,
    pub rewards: Option<String>,
    pub coupons: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWallet {
    coupons: Option<RawCoupons>,
    epic_rewards: Option<RawBalance>,
    wallet: Option<RawBalance>,
}

#[derive(Deserialize)]
struct RawCoupons {
    count: Option<u32>,
}

#[derive(Deserialize)]
struct RawBalance {
    #[serde(rename = "__typename", default)]
    typename: String,
    balance: Option<String>,
}

impl RawBalance {
    /// `EpicWalletSupported` / `EpicRewardsSupported`: anything else means not offered here.
    fn shown(self) -> Option<String> {
        self.typename
            .ends_with("Supported")
            .then_some(self.balance)
            .flatten()
            .filter(|b| !b.is_empty())
    }
}

/// `GET egs-platform-service .../private/egs/account/wallet` (needs the wallet and
/// reward-accounts read permissions, which the launcher token has).
pub async fn wallet_summary(
    client: &Client,
    access: &Secret,
    locale: &str,
) -> Result<WalletSummary, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/api/v1/private/egs/account/wallet",
            client.hosts().platform
        ))
        .query(&[("locale", locale), ("store", "EGS")])
        .bearer_auth(access.expose());
    let raw: RawWallet = client.json(req, Retry::Idempotent).await?;
    tracing::debug!(
        wallet = raw
            .wallet
            .as_ref()
            .map(|w| w.typename.as_str())
            .unwrap_or("-"),
        rewards = raw
            .epic_rewards
            .as_ref()
            .map(|w| w.typename.as_str())
            .unwrap_or("-"),
        "wallet kinds"
    );
    Ok(WalletSummary {
        balance: raw.wallet.and_then(RawBalance::shown),
        rewards: raw.epic_rewards.and_then(RawBalance::shown),
        coupons: raw.coupons.and_then(|c| c.count),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedeemError {
    AlreadyRedeemed,
    /// A coupon code: it applies at checkout, not here.
    Coupon,
    Invalid,
    /// Too many attempts; Epic asks to wait.
    TooMany,
    Other(String),
}

impl std::fmt::Display for RedeemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AlreadyRedeemed => "You already redeemed this code.",
            Self::Coupon => "This is a coupon code. Use it at checkout on Epic's store.",
            Self::Invalid => "That code isn't valid. Check it and try again.",
            Self::TooMany => "Too many attempts. Wait a few minutes and try again.",
            Self::Other(m) => m,
        })
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RedeemDetail {
    #[serde(default)]
    pub entitlement_name: String,
    #[serde(default)]
    pub namespace: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Redeemed {
    #[serde(default)]
    pub details: Vec<RedeemDetail>,
}

/// Letters and digits only, upper-cased (codes are often typed with dashes or spaces).
pub fn normalize_code(code: &str) -> Option<String> {
    let c: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (4..=64).contains(&c.len()).then_some(c)
}

/// `POST fulfillment .../accounts/<id>/codes/<CODE>`. Sent once, never retried (a code is
/// single-use), and the code never reaches a log line.
pub async fn redeem_code(
    client: &Client,
    access: &Secret,
    account_id: &str,
    code: &str,
) -> Result<Redeemed, Result<RedeemError, ApiError>> {
    let code = normalize_code(code).ok_or(Ok(RedeemError::Invalid))?;
    let req = client
        .http()
        .post(format!(
            "{}/fulfillment/api/public/accounts/{}/codes/{}",
            client.hosts().fulfillment,
            account_id,
            code
        ))
        .bearer_auth(access.expose());
    match client.json::<Redeemed>(req, Retry::Never).await {
        Ok(r) => Ok(r),
        Err(e) if e.is_auth_invalid() || (e.is_transient() && e.status() != Some(429)) => {
            Err(Err(e))
        }
        Err(e) => {
            let code = e.epic_code().unwrap_or("");
            let mapped = if code.contains("multiple_redemptions_not_allowed")
                || code.contains("code_used")
            {
                RedeemError::AlreadyRedeemed
            } else if code.contains("coupon_code") {
                RedeemError::Coupon
            } else if code.contains("code_not_found")
                || code.contains("not_found")
                || code.contains("invalid")
            {
                RedeemError::Invalid
            } else if e.status() == Some(429) || code.contains("throttle") {
                RedeemError::TooMany
            } else if e.status().is_some() {
                RedeemError::Other("Epic couldn't redeem this code.".into())
            } else {
                return Err(Err(e));
            };
            Err(Ok(mapped))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn wallet_summary_reads_supported_balances_only() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/private/egs/account/wallet"))
            .and(query_param("store", "EGS"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "__typename": "Wallet",
                "coupons": { "count": 2 },
                "epicRewards": { "__typename": "EpicRewardsSupported", "balance": "€9.63", "rawBalance": 963 },
                "wallet": { "__typename": "EpicWalletUnsupported" }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let w = wallet_summary(&c, &Secret::new("tok"), "en-US")
            .await
            .unwrap();
        assert_eq!(
            w,
            WalletSummary {
                balance: None,
                rewards: Some("€9.63".into()),
                coupons: Some(2)
            }
        );
    }

    #[test]
    fn codes_are_normalized() {
        assert_eq!(
            normalize_code(" abcd-1234-efgh ").as_deref(),
            Some("ABCD1234EFGH")
        );
        assert_eq!(normalize_code("../x"), None);
        assert_eq!(normalize_code("ab"), None);
    }

    #[tokio::test]
    async fn redeem_maps_epic_errors_and_never_retries() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/fulfillment/api/public/accounts/acc/codes/USEDCODE"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.coderedemption.multiple_redemptions_not_allowed",
                "numericErrorCode": 19025
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/fulfillment/api/public/accounts/acc/codes/GOODCODE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offerId": "o", "accountId": "acc",
                "details": [{ "entitlementId": "e", "entitlementName": "Game", "itemId": "i", "namespace": "ns" }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/fulfillment/api/public/accounts/acc/codes/BUSYCODE"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let t = Secret::new("tok");
        assert_eq!(
            redeem_code(&c, &t, "acc", "used-code")
                .await
                .unwrap_err()
                .unwrap(),
            RedeemError::AlreadyRedeemed
        );
        let ok = redeem_code(&c, &t, "acc", "goodcode").await.unwrap();
        assert_eq!(ok.details[0].entitlement_name, "Game");
        assert!(
            redeem_code(&c, &t, "acc", "busycode")
                .await
                .unwrap_err()
                .is_err(),
            "transient: caller decides"
        );
        assert_eq!(
            redeem_code(&c, &t, "acc", "x").await.unwrap_err().unwrap(),
            RedeemError::Invalid
        );
    }
}
