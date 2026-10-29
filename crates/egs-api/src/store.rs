//! Store reads (docs/protocol/store.md): search/browse, the free-games feed and offer
//! details. All unauthenticated. Queries use GraphQL variables, so user text is never
//! pasted into a query. A bot check (HTML or 403 without Epic's error body) becomes
//! [`ApiError::Challenge`]; it is never solved or worked around (ADR 0005).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::http::{Client, Retry};
use crate::ApiError;

// ---- public types -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    /// Minor units (cents).
    pub original: i64,
    pub current: i64,
    pub currency: String,
    pub fmt_original: String,
    pub fmt_current: String,
}

impl Price {
    pub fn discount_percent(&self) -> u32 {
        if self.original <= 0 || self.current >= self.original {
            return 0;
        }
        // Rounded to the nearest percent, as the store shows it.
        (((self.original - self.current) * 100 + self.original / 2) / self.original).clamp(0, 100)
            as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub seller: String,
    /// Store page slug (`store.epicgames.com/p/<slug>`).
    pub slug: Option<String>,
    pub offer_type: String,
    pub effective_date: Option<String>,
    pub image_tall: Option<String>,
    pub image_wide: Option<String>,
    pub price: Option<Price>,
    /// Catalog item ids the offer grants (ownership is checked against these).
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub offers: Vec<Offer>,
    pub total: u32,
}

/// A `searchStore` request. Empty fields are left out.
#[derive(Debug, Clone, Default)]
pub struct Browse {
    pub keywords: Option<String>,
    pub category: Option<String>,
    pub on_sale: bool,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    /// e.g. `[,2026-10-29T23:59:59.999Z]` (released up to then).
    pub release_date: Option<String>,
    pub start: u32,
    pub count: u32,
    pub country: String,
    pub locale: String,
}

/// Games, bundles and editors: what the store's own "browse" shows for games.
pub const GAMES_CATEGORY: &str = "games/edition/base|bundles/games|editors";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeGame {
    pub offer: Offer,
    pub starts: String,
    pub ends: String,
    /// Free right now (else: coming next).
    pub now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferDetail {
    pub offer: Offer,
    pub description: String,
    /// Markdown with Epic's `<!--block-->` comments; render as text, never as HTML.
    pub long_description: String,
    pub developer: String,
    pub publisher: String,
    pub release_date: Option<String>,
    /// `(group, name)`, e.g. `("genre", "Horror")`.
    pub tags: Vec<(String, String)>,
}

// ---- wire types ---------------------------------------------------------------------

#[derive(Deserialize)]
struct Gql<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    #[serde(default)]
    message: String,
}

#[derive(Deserialize)]
struct CatalogRoot<T> {
    #[serde(rename = "Catalog")]
    catalog: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchStore {
    search_store: RawPage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogOffer {
    catalog_offer: Option<RawOffer>,
}

#[derive(Deserialize)]
struct RawPage {
    #[serde(default)]
    elements: Vec<RawOffer>,
    paging: Option<Paging>,
}

#[derive(Deserialize)]
struct Paging {
    #[serde(default)]
    total: u32,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawOffer {
    title: String,
    id: String,
    namespace: String,
    offer_type: Option<String>,
    effective_date: Option<String>,
    product_slug: Option<String>,
    url_slug: Option<String>,
    seller: Option<Named>,
    items: Vec<Item>,
    key_images: Vec<KeyImage>,
    catalog_ns: Option<CatalogNs>,
    offer_mappings: Option<Vec<Mapping>>,
    price: Option<RawPrice>,
    promotions: Option<Promotions>,
    description: Option<String>,
    long_description: Option<String>,
    developer_display_name: Option<String>,
    publisher_display_name: Option<String>,
    release_date: Option<String>,
    tags: Option<Vec<Tag>>,
}

#[derive(Deserialize, Default)]
struct Named {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Item {
    id: String,
}

#[derive(Deserialize)]
struct KeyImage {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize, Default)]
struct CatalogNs {
    mappings: Option<Vec<Mapping>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Mapping {
    #[serde(default)]
    page_slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPrice {
    total_price: TotalPrice,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TotalPrice {
    #[serde(default)]
    discount_price: i64,
    #[serde(default)]
    original_price: i64,
    #[serde(default)]
    currency_code: String,
    fmt_price: Option<FmtPrice>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FmtPrice {
    original_price: String,
    discount_price: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Promotions {
    promotional_offers: Vec<PromoGroup>,
    upcoming_promotional_offers: Vec<PromoGroup>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PromoGroup {
    promotional_offers: Vec<Promo>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Promo {
    start_date: String,
    end_date: String,
    discount_setting: Option<DiscountSetting>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DiscountSetting {
    discount_percentage: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Tag {
    name: String,
    group_name: Option<String>,
}

fn pick(images: &[KeyImage], kinds: &[&str]) -> Option<String> {
    kinds.iter().find_map(|k| {
        images
            .iter()
            .find(|i| i.kind == *k && !i.url.is_empty())
            .map(|i| i.url.clone())
    })
}

impl RawOffer {
    fn slug(&self) -> Option<String> {
        let from = |m: &Option<Vec<Mapping>>| {
            m.as_ref()
                .and_then(|v| v.iter().find(|x| !x.page_slug.is_empty()))
                .map(|x| x.page_slug.clone())
        };
        self.catalog_ns
            .as_ref()
            .and_then(|c| from(&c.mappings))
            .or_else(|| from(&self.offer_mappings))
            .or_else(|| {
                self.product_slug
                    .as_deref()
                    .map(|s| s.trim_end_matches("/home").to_owned())
                    .filter(|s| !s.is_empty() && !s.contains('/'))
            })
    }

    fn offer(&self) -> Offer {
        let _ = &self.url_slug; // kept for diagnostics; often an internal id
        Offer {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            title: self.title.clone(),
            seller: self
                .seller
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            slug: self.slug(),
            offer_type: self.offer_type.clone().unwrap_or_default(),
            effective_date: self.effective_date.clone(),
            image_tall: pick(
                &self.key_images,
                &[
                    "OfferImageTall",
                    "DieselStoreFrontTall",
                    "Thumbnail",
                    "CodeRedemption_340x440",
                ],
            ),
            image_wide: pick(
                &self.key_images,
                &["OfferImageWide", "DieselStoreFrontWide", "featuredMedia"],
            ),
            price: self.price.as_ref().map(|p| {
                let t = &p.total_price;
                let fmt = t.fmt_price.as_ref();
                Price {
                    original: t.original_price,
                    current: t.discount_price,
                    currency: t.currency_code.clone(),
                    fmt_original: fmt.map(|f| f.original_price.clone()).unwrap_or_default(),
                    fmt_current: fmt.map(|f| f.discount_price.clone()).unwrap_or_default(),
                }
            }),
            item_ids: self
                .items
                .iter()
                .filter(|i| !i.id.is_empty())
                .map(|i| i.id.clone())
                .collect(),
        }
    }
}

// ---- requests -----------------------------------------------------------------------

const OFFER_FIELDS: &str = "title id namespace offerType effectiveDate productSlug urlSlug \
    seller { name } items { id } keyImages { type url } \
    catalogNs { mappings(pageType: \"productHome\") { pageSlug } } \
    price(country: $country) { totalPrice { discountPrice originalPrice currencyCode \
    fmtPrice(locale: $locale) { originalPrice discountPrice } } }";

/// POST a GraphQL query to the store; bot checks become [`ApiError::Challenge`].
async fn graphql<T: DeserializeOwned>(
    client: &Client,
    query: &str,
    variables: serde_json::Value,
) -> Result<T, ApiError> {
    let req = client
        .http()
        .post(format!("{}/graphql", client.hosts().store))
        .json(&serde_json::json!({ "query": query, "variables": variables }));
    let bytes = read(client, req).await?;
    let gql: Gql<T> = serde_json::from_slice(&bytes).map_err(|_| ApiError::Challenge)?;
    match gql.data {
        Some(d) => Ok(d),
        None => Err(ApiError::Decode(
            gql.errors
                .into_iter()
                .next()
                .map(|e| e.message)
                .unwrap_or_else(|| "no data".into()),
        )),
    }
}

/// Send and read the body, turning bot-check answers into [`ApiError::Challenge`].
async fn read(client: &Client, req: reqwest::RequestBuilder) -> Result<Vec<u8>, ApiError> {
    let resp = match client.send(req, Retry::Idempotent).await {
        Ok(r) => r,
        // 403 without Epic's JSON error body is a bot wall, not an API refusal.
        Err(ApiError::Http {
            status: 403 | 429,
            epic: None,
            ..
        }) => return Err(ApiError::Challenge),
        Err(e) => return Err(e),
    };
    let html = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));
    if html {
        return Err(ApiError::Challenge);
    }
    Ok(resp.bytes().await?.to_vec())
}

pub async fn search(client: &Client, b: &Browse) -> Result<Page, ApiError> {
    let query = format!(
        "query searchStore($count: Int, $start: Int, $country: String!, $locale: String, \
         $category: String, $keywords: String, $onSale: Boolean, $sortBy: String, \
         $sortDir: String, $releaseDate: String) {{ Catalog {{ searchStore(count: $count, \
         start: $start, country: $country, locale: $locale, category: $category, \
         keywords: $keywords, onSale: $onSale, sortBy: $sortBy, sortDir: $sortDir, \
         releaseDate: $releaseDate) {{ elements {{ {OFFER_FIELDS} }} paging {{ count total }} }} }} }}"
    );
    let vars = serde_json::json!({
        "count": b.count.clamp(1, 100),
        "start": b.start,
        "country": b.country,
        "locale": b.locale,
        "category": b.category,
        "keywords": b.keywords.as_deref().map(str::trim).filter(|k| !k.is_empty()),
        "onSale": b.on_sale.then_some(true),
        "sortBy": b.sort_by,
        "sortDir": b.sort_dir,
        "releaseDate": b.release_date,
    });
    let root: CatalogRoot<SearchStore> = graphql(client, &query, vars).await?;
    let page = root.catalog.search_store;
    Ok(Page {
        total: page.paging.map(|p| p.total).unwrap_or(0),
        offers: page
            .elements
            .iter()
            .filter(|e| !e.id.is_empty())
            .map(RawOffer::offer)
            .collect(),
    })
}

pub async fn offer_detail(
    client: &Client,
    namespace: &str,
    id: &str,
    country: &str,
    locale: &str,
) -> Result<OfferDetail, ApiError> {
    let query = format!(
        "query catalogOffer($namespace: String!, $id: String!, $country: String!, $locale: String) \
         {{ Catalog {{ catalogOffer(namespace: $namespace, id: $id, locale: $locale) {{ {OFFER_FIELDS} \
         description longDescription developerDisplayName publisherDisplayName releaseDate \
         tags {{ name groupName }} }} }} }}"
    );
    let vars = serde_json::json!({ "namespace": namespace, "id": id, "country": country, "locale": locale });
    let root: CatalogRoot<CatalogOffer> = graphql(client, &query, vars).await?;
    let raw = root
        .catalog
        .catalog_offer
        .ok_or_else(|| ApiError::Decode("offer not found".into()))?;
    Ok(OfferDetail {
        offer: raw.offer(),
        description: raw.description.clone().unwrap_or_default(),
        long_description: raw.long_description.clone().unwrap_or_default(),
        developer: raw.developer_display_name.clone().unwrap_or_default(),
        publisher: raw.publisher_display_name.clone().unwrap_or_default(),
        release_date: raw.release_date.clone(),
        tags: raw
            .tags
            .iter()
            .flatten()
            .filter(|t| !t.name.is_empty())
            .map(|t| (t.group_name.clone().unwrap_or_default(), t.name.clone()))
            .collect(),
    })
}

/// The weekly free-games feed: free now, and coming next. `now` is an ISO-8601 UTC time.
pub async fn free_games(
    client: &Client,
    country: &str,
    locale: &str,
    now: &str,
) -> Result<Vec<FreeGame>, ApiError> {
    let req = client
        .http()
        .get(format!(
            "{}/freeGamesPromotions",
            client.hosts().store_static
        ))
        .query(&[
            ("locale", locale),
            ("country", country),
            ("allowCountries", country),
        ]);
    let bytes = read(client, req).await?;
    parse_free_games(&bytes, now)
}

pub fn parse_free_games(bytes: &[u8], now: &str) -> Result<Vec<FreeGame>, ApiError> {
    #[derive(Deserialize)]
    struct Root {
        data: CatalogRoot<SearchStore>,
    }
    let root: Root = serde_json::from_slice(bytes).map_err(|e| ApiError::Decode(e.to_string()))?;
    let mut out = Vec::new();
    for e in &root.data.catalog.search_store.elements {
        let Some(p) = &e.promotions else { continue };
        let free = |groups: &[PromoGroup]| {
            groups
                .iter()
                .flat_map(|g| &g.promotional_offers)
                .find(|o| {
                    o.discount_setting
                        .as_ref()
                        .is_some_and(|d| d.discount_percentage == 0)
                })
                .map(|o| (o.start_date.clone(), o.end_date.clone()))
        };
        if let Some((starts, ends)) = free(&p.promotional_offers) {
            if starts.as_str() <= now && now < ends.as_str() {
                out.push(FreeGame {
                    offer: e.offer(),
                    starts,
                    ends,
                    now: true,
                });
                continue;
            }
        }
        if let Some((starts, ends)) = free(&p.upcoming_promotional_offers) {
            if now < ends.as_str() {
                out.push(FreeGame {
                    offer: e.offer(),
                    starts,
                    ends,
                    now: false,
                });
            }
        }
    }
    out.sort_by(|a, b| b.now.cmp(&a.now).then(a.starts.cmp(&b.starts)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_client;
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn element(id: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut e = serde_json::json!({
            "title": format!("Game {id}"), "id": id, "namespace": "ns",
            "offerType": "BASE_GAME", "effectiveDate": "2026-01-01T00:00:00.000Z",
            "productSlug": null, "urlSlug": "x",
            "seller": { "name": "Studio" },
            "items": [ { "id": "item-1" } ],
            "keyImages": [
                { "type": "Thumbnail", "url": "https://cdn1.epicgames.com/t.jpg" },
                { "type": "OfferImageTall", "url": "https://cdn1.epicgames.com/tall.jpg" },
                { "type": "OfferImageWide", "url": "https://cdn1.epicgames.com/wide.jpg" }
            ],
            "catalogNs": { "mappings": [ { "pageSlug": format!("game-{id}") } ] },
            "price": { "totalPrice": { "discountPrice": 1499, "originalPrice": 2999, "currencyCode": "USD",
                "fmtPrice": { "originalPrice": "$29.99", "discountPrice": "$14.99" } } }
        });
        if let (Some(o), Some(x)) = (e.as_object_mut(), extra.as_object()) {
            for (k, v) in x {
                o.insert(k.clone(), v.clone());
            }
        }
        e
    }

    #[tokio::test]
    async fn search_sends_variables_and_maps_offers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "keywords": "hollow \"knight\"", "country": "US", "onSale": null, "count": 2 }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "Catalog": { "searchStore": {
                    "elements": [element("a", serde_json::json!({})), element("", serde_json::json!({}))],
                    "paging": { "count": 2, "total": 41 } } } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let page = search(
            &c,
            &Browse {
                keywords: Some("  hollow \"knight\" ".into()),
                count: 2,
                country: "US".into(),
                locale: "en-US".into(),
                ..Browse::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 41);
        assert_eq!(page.offers.len(), 1, "elements without an id are dropped");
        let o = &page.offers[0];
        assert_eq!(o.slug.as_deref(), Some("game-a"));
        assert_eq!(o.item_ids, ["item-1"]);
        assert_eq!(
            o.image_tall.as_deref(),
            Some("https://cdn1.epicgames.com/tall.jpg")
        );
        let p = o.price.as_ref().unwrap();
        assert_eq!(
            (p.discount_percent(), p.fmt_current.as_str()),
            (50, "$14.99")
        );
    }

    #[tokio::test]
    async fn bot_checks_become_challenge() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html>verify you are human</html>"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/freeGamesPromotions"))
            .respond_with(ResponseTemplate::new(403).set_body_string("<html>blocked</html>"))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let b = Browse {
            country: "US".into(),
            ..Browse::default()
        };
        assert!(matches!(search(&c, &b).await, Err(ApiError::Challenge)));
        assert!(matches!(
            free_games(&c, "US", "en-US", "2026-10-29T00:00:00.000Z").await,
            Err(ApiError::Challenge)
        ));
    }

    #[tokio::test]
    async fn graphql_errors_are_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": null, "errors": [ { "message": "bad variable" } ]
            })))
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let r = offer_detail(&c, "ns", "id", "US", "en-US").await;
        assert!(matches!(r, Err(ApiError::Decode(m)) if m == "bad variable"));
    }

    #[tokio::test]
    async fn free_games_query_and_windows() {
        let promo = |start: &str, end: &str, pct: i64| {
            serde_json::json!([{ "promotionalOffers": [{ "startDate": start, "endDate": end,
                "discountSetting": { "discountType": "PERCENTAGE", "discountPercentage": pct } }] }])
        };
        let body = serde_json::json!({ "data": { "Catalog": { "searchStore": { "elements": [
            element("now", serde_json::json!({ "promotions": {
                "promotionalOffers": promo("2026-10-23T15:00:00.000Z", "2026-10-30T15:00:00.000Z", 0),
                "upcomingPromotionalOffers": [] } })),
            element("next", serde_json::json!({ "promotions": {
                "promotionalOffers": [],
                "upcomingPromotionalOffers": promo("2026-10-30T15:00:00.000Z", "2026-11-06T15:00:00.000Z", 0) } })),
            element("sale", serde_json::json!({ "promotions": {
                "promotionalOffers": promo("2026-10-23T15:00:00.000Z", "2026-10-30T15:00:00.000Z", 50),
                "upcomingPromotionalOffers": [] } })),
            element("plain", serde_json::json!({ "promotions": null }))
        ], "paging": { "total": 4 } } } } });
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/freeGamesPromotions"))
            .and(query_param("country", "DE"))
            .and(query_param("allowCountries", "DE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
        let c = test_client(&server.uri());
        let games = free_games(&c, "DE", "de-DE", "2026-10-29T12:00:00.000Z")
            .await
            .unwrap();
        let ids: Vec<(&str, bool)> = games.iter().map(|g| (g.offer.id.as_str(), g.now)).collect();
        assert_eq!(ids, [("now", true), ("next", false)]);
    }

    #[test]
    fn free_game_that_ended_is_dropped() {
        let body = serde_json::json!({ "data": { "Catalog": { "searchStore": { "elements": [
            element("old", serde_json::json!({ "promotions": {
                "promotionalOffers": [{ "promotionalOffers": [{ "startDate": "2026-10-01T00:00:00.000Z",
                    "endDate": "2026-10-08T00:00:00.000Z", "discountSetting": { "discountPercentage": 0 } }] }],
                "upcomingPromotionalOffers": [] } }))
        ] } } } });
        let games =
            parse_free_games(body.to_string().as_bytes(), "2026-10-29T00:00:00.000Z").unwrap();
        assert!(games.is_empty());
    }
}
