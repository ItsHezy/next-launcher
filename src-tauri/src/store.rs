//! Store (Phase 6): browse, search, free games, product details; wishlist, cart and
//! checkout open on Epic's own pages in the signed-in web window.
//! Spec: docs/protocol/store.md, ADR 0005. Requests: `egs-api::store`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use egs_api::store::{self as api, Browse, Offer, GAMES_CATEGORY};
use egs_cache::Owned;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::auth::AuthState;
use crate::commands::{settings_value, AppState, UiError};

const PAGE: u32 = 30;
const BROWSE_TTL: i64 = 10 * 60;
const PRODUCT_TTL: i64 = 60 * 60;
const FREE_MAX_TTL: i64 = 6 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StorePrice {
    pub fmt_original: String,
    pub fmt_current: String,
    /// Percent off, 0 when not discounted.
    pub discount: f64,
    pub free: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StoreOffer {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub seller: String,
    pub slug: Option<String>,
    /// Resized by Epic's CDN for the grid (3:4).
    pub image_tall: Option<String>,
    /// Resized for headers (16:9).
    pub image_wide: Option<String>,
    pub price: Option<StorePrice>,
    /// Every item it grants is already in the library.
    pub owned: bool,
    /// Free-games feed: the giveaway window (ISO-8601 UTC).
    pub free_from: Option<String>,
    pub free_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StorePage {
    pub offers: Vec<StoreOffer>,
    pub start: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StoreFeatured {
    pub free: Vec<StoreOffer>,
    pub on_sale: Vec<StoreOffer>,
    pub new_releases: Vec<StoreOffer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StoreProduct {
    pub offer: StoreOffer,
    pub description: String,
    /// Markdown; the UI renders it as text with simple formatting, never as HTML.
    pub long_description: String,
    pub developer: String,
    pub publisher: String,
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StoreChip {
    Free,
    OnSale,
    NewReleases,
}

/// Epic store pages opened in the signed-in web window.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoreTarget {
    Home,
    Wishlist,
    Cart,
    Product {
        slug: String,
    },
    /// Epic's checkout for one offer: the user reviews and pays there.
    Checkout {
        namespace: String,
        #[serde(rename = "offerId")]
        offer_id: String,
    },
    Search {
        query: String,
    },
}

fn err(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.into(),
        message: message.into(),
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Country and locale for store requests: the account's country, the app language.
fn region(state: &AppState) -> (String, String) {
    let country = state
        .cache
        .signed_in_country()
        .ok()
        .flatten()
        .filter(|c| c.len() == 2)
        .unwrap_or_else(|| "US".into());
    let locale = settings_value(&state.cache, "general.language").unwrap_or_else(|| "en-US".into());
    (country.to_ascii_uppercase(), locale)
}

/// Ask Epic's image CDN for a resized copy (docs/protocol/catalog-and-images.md).
fn resized(url: Option<String>, w: u32, h: u32) -> Option<String> {
    let url = url?;
    let ok = url.starts_with("https://cdn1.epicgames.com/")
        || url.starts_with("https://cdn2.unrealengine.com/");
    if !ok {
        return None; // the UI only loads images from Epic's CDN hosts
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    Some(format!("{url}{sep}resize=1&w={w}&h={h}&quality=medium"))
}

/// Owned when every item it grants is in the library, or, for a base game, when the
/// library has a base game in the same namespace (store and launcher item ids differ).
fn is_owned(o: &Offer, owned: &Owned) -> bool {
    let items = !o.item_ids.is_empty() && o.item_ids.iter().all(|i| owned.items.contains(i));
    items || (o.offer_type == "BASE_GAME" && owned.base_namespaces.contains(&o.namespace))
}

fn to_ui(o: Offer, owned: &Owned) -> StoreOffer {
    let price = o.price.as_ref().map(|p| StorePrice {
        fmt_original: p.fmt_original.clone(),
        fmt_current: p.fmt_current.clone(),
        discount: f64::from(p.discount_percent()),
        free: p.current == 0,
    });
    StoreOffer {
        owned: is_owned(&o, owned),
        id: o.id,
        namespace: o.namespace,
        title: o.title,
        seller: o.seller,
        slug: o.slug,
        image_tall: resized(o.image_tall, 360, 480),
        image_wide: resized(o.image_wide, 1280, 720),
        price,
        free_from: None,
        free_until: None,
    }
}

/// Cached JSON by key, else fetch and store for `ttl` seconds.
async fn cached<T, F, Fut>(state: &AppState, key: &str, ttl: i64, fetch: F) -> Result<T, UiError>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(T, i64), egs_api::ApiError>>,
{
    if let Some(body) = state.cache.store_get(key)? {
        if let Ok(v) = serde_json::from_str(&body) {
            return Ok(v);
        }
    }
    let (value, ttl_override) = fetch().await.map_err(|e| {
        // Logged once per session (ADR 0005); the view offers Epic's own page instead.
        static WARNED: AtomicBool = AtomicBool::new(false);
        if matches!(e, egs_api::ApiError::Challenge) && !WARNED.swap(true, Ordering::Relaxed) {
            tracing::warn!("store answered with a bot check; using Epic's own page instead");
        }
        UiError::from(e)
    })?;
    if let Ok(json) = serde_json::to_string(&value) {
        let _ = state.cache.store_put(
            key,
            &json,
            if ttl_override > 0 { ttl_override } else { ttl },
        );
    }
    Ok(value)
}

fn browse_for(chip: StoreChip, start: u32, count: u32, country: &str, locale: &str) -> Browse {
    let mut b = Browse {
        category: Some(GAMES_CATEGORY.into()),
        start,
        count,
        country: country.into(),
        locale: locale.into(),
        ..Browse::default()
    };
    match chip {
        StoreChip::OnSale => {
            b.on_sale = true;
            b.sort_by = Some("currentPrice".into());
            b.sort_dir = Some("DESC".into());
        }
        StoreChip::NewReleases | StoreChip::Free => {
            b.sort_by = Some("releaseDate".into());
            b.sort_dir = Some("DESC".into());
            b.release_date = Some(format!(
                "[,{}]",
                egs_api::launch::iso8601(now_secs() * 1000)
            ));
        }
    }
    b
}

async fn free_games(state: &AppState, auth: &AuthState) -> Result<Vec<StoreOffer>, UiError> {
    let (country, locale) = region(state);
    let owned = state.cache.owned_items().unwrap_or_default();
    let key = format!("free:{country}:{locale}");
    let api = auth.session.api().clone();
    let games: Vec<api::FreeGame> = cached(state, &key, FREE_MAX_TTL, || async move {
        let now = egs_api::launch::iso8601(now_secs() * 1000);
        let games = api::free_games(&api, &country, &locale, &now).await?;
        // Cache until the next giveaway boundary, at most 6 h.
        let next = games
            .iter()
            .flat_map(|g| [g.starts.as_str(), g.ends.as_str()])
            .filter(|t| *t > now.as_str())
            .min()
            .and_then(crate::library::parse_iso8601)
            .map(|t| (t - now_secs()).clamp(60, FREE_MAX_TTL))
            .unwrap_or(FREE_MAX_TTL);
        Ok((games, next))
    })
    .await?;
    Ok(games
        .into_iter()
        .map(|g| {
            let mut o = to_ui(g.offer, &owned);
            o.free_from = Some(g.starts);
            o.free_until = Some(g.ends);
            o
        })
        .collect())
}

/// Free right now (id, title), for the weekly free-games notification. Uses the cache.
pub async fn free_now(app: &AppHandle) -> Result<Vec<(String, String)>, UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let now = egs_api::launch::iso8601(now_secs() * 1000);
    Ok(free_games(&state, &auth)
        .await?
        .into_iter()
        .filter(|o| o.free_from.as_deref().is_some_and(|f| f <= now.as_str()))
        .map(|o| (o.id, o.title))
        .collect())
}

async fn browse_page(
    state: &AppState,
    auth: &AuthState,
    chip: StoreChip,
    start: u32,
    count: u32,
) -> Result<StorePage, UiError> {
    let (country, locale) = region(state);
    let owned = state.cache.owned_items().unwrap_or_default();
    let key = format!("browse:{chip:?}:{start}:{count}:{country}:{locale}");
    let api = auth.session.api().clone();
    let b = browse_for(chip, start, count, &country, &locale);
    let page: api::Page = cached(state, &key, BROWSE_TTL, || async move {
        Ok((api::search(&api, &b).await?, 0))
    })
    .await?;
    Ok(StorePage {
        start: f64::from(start),
        total: f64::from(page.total),
        offers: page.offers.into_iter().map(|o| to_ui(o, &owned)).collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn store_featured(
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
) -> Result<StoreFeatured, UiError> {
    let (free, sale, new) = tokio::join!(
        free_games(&state, &auth),
        browse_page(&state, &auth, StoreChip::OnSale, 0, 12),
        browse_page(&state, &auth, StoreChip::NewReleases, 0, 12),
    );
    // A bot check on any part means the whole store view moves to Epic's page.
    for r in [&sale, &new] {
        if let Err(e) = r {
            if e.code == "challenge" {
                return Err(e.clone());
            }
        }
    }
    if let Err(e) = &free {
        if e.code == "challenge" {
            return Err(e.clone());
        }
    }
    if free.is_err() && sale.is_err() && new.is_err() {
        return Err(sale
            .err()
            .unwrap_or_else(|| err("epic", "The store is unavailable.")));
    }
    Ok(StoreFeatured {
        free: free.unwrap_or_default(),
        on_sale: sale.map(|p| p.offers).unwrap_or_default(),
        new_releases: new.map(|p| p.offers).unwrap_or_default(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn store_browse(
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
    chip: StoreChip,
    start: f64,
) -> Result<StorePage, UiError> {
    if chip == StoreChip::Free {
        let offers = free_games(&state, &auth).await?;
        return Ok(StorePage {
            start: 0.0,
            total: offers.len() as f64,
            offers,
        });
    }
    browse_page(&state, &auth, chip, start.max(0.0) as u32, PAGE).await
}

#[tauri::command]
#[specta::specta]
pub async fn store_search(
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
    query: String,
    start: f64,
) -> Result<StorePage, UiError> {
    let query: String = query.trim().chars().take(100).collect();
    if query.is_empty() {
        return Ok(StorePage {
            offers: vec![],
            start: 0.0,
            total: 0.0,
        });
    }
    let (country, locale) = region(&state);
    let owned = state.cache.owned_items().unwrap_or_default();
    let start = start.max(0.0) as u32;
    let key = format!("search:{}:{start}:{country}:{locale}", query.to_lowercase());
    let api = auth.session.api().clone();
    let b = Browse {
        keywords: Some(query),
        category: Some(GAMES_CATEGORY.into()),
        start,
        count: PAGE,
        country,
        locale,
        ..Browse::default()
    };
    let page: api::Page = cached(&state, &key, BROWSE_TTL, || async move {
        Ok((api::search(&api, &b).await?, 0))
    })
    .await?;
    Ok(StorePage {
        start: f64::from(start),
        total: f64::from(page.total),
        offers: page.offers.into_iter().map(|o| to_ui(o, &owned)).collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn store_product(
    state: State<'_, AppState>,
    auth: State<'_, AuthState>,
    namespace: String,
    id: String,
) -> Result<StoreProduct, UiError> {
    if !safe_id(&namespace) || !safe_id(&id) {
        return Err(err("invalid", "That isn't a store item."));
    }
    let (country, locale) = region(&state);
    let owned = state.cache.owned_items().unwrap_or_default();
    let key = format!("offer:{namespace}:{id}:{country}:{locale}");
    let api = auth.session.api().clone();
    let d: api::OfferDetail = cached(&state, &key, PRODUCT_TTL, || async move {
        Ok((
            api::offer_detail(&api, &namespace, &id, &country, &locale).await?,
            0,
        ))
    })
    .await?;
    let by_group = |g: &str| -> Vec<String> {
        d.tags
            .iter()
            .filter(|(group, _)| group == g)
            .map(|(_, n)| n.clone())
            .collect()
    };
    Ok(StoreProduct {
        genres: by_group("genre"),
        features: by_group("feature"),
        description: d.description.clone(),
        long_description: d.long_description.clone(),
        developer: d.developer.clone(),
        publisher: d.publisher.clone(),
        release_date: d.release_date.clone(),
        offer: to_ui(d.offer, &owned),
    })
}

/// Dev tooling (`--dev-action store:<search text>`): the UI's store calls, counts only.
pub async fn dev_check(app: &AppHandle, query: &str) -> Result<String, UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let t = std::time::Instant::now();
    let f = store_featured(state.clone(), auth.clone()).await?;
    let featured_ms = t.elapsed().as_millis();
    let t = std::time::Instant::now();
    let s = store_search(state.clone(), auth.clone(), query.to_owned(), 0.0).await?;
    let search_ms = t.elapsed().as_millis();
    let first = f.new_releases.first().or(f.on_sale.first()).cloned();
    let detail = match first {
        Some(o) => {
            let t = std::time::Instant::now();
            let p = store_product(state.clone(), auth.clone(), o.namespace, o.id).await?;
            format!(
                "product ok ({} ms, {} chars, {} genres)",
                t.elapsed().as_millis(),
                p.long_description.len(),
                p.genres.len()
            )
        }
        None => "no product".into(),
    };
    let t = std::time::Instant::now();
    let _ = store_featured(state.clone(), auth.clone()).await?;
    let cached_ms = t.elapsed().as_millis();
    Ok(format!(
        "featured: {} free ({} now), {} on sale, {} new in {featured_ms} ms (cached {cached_ms} ms);          search: {} of {} in {search_ms} ms; {detail}; owned marked: {}",
        f.free.len(),
        f.free.iter().filter(|o| o.free_from.as_deref() <= Some(egs_api::launch::iso8601(now_secs() * 1000).as_str())).count(),
        f.on_sale.len(),
        f.new_releases.len(),
        s.offers.len(),
        s.total,
        f.free.iter().chain(&f.on_sale).chain(&f.new_releases).chain(&s.offers).filter(|o| o.owned).count(),
    ))
}

/// Dev tooling (`--dev-action open:<wishlist|cart|product=<slug>|checkout=<ns>/<offer>>`).
pub async fn dev_open(app: &AppHandle, spec: &str) -> Result<String, UiError> {
    let target = match spec.split_once('=') {
        Some(("product", slug)) => StoreTarget::Product { slug: slug.into() },
        Some(("checkout", rest)) => match rest.split_once('/') {
            Some((namespace, offer_id)) => StoreTarget::Checkout {
                namespace: namespace.into(),
                offer_id: offer_id.into(),
            },
            None => return Err(err("dev", "checkout=<ns>/<offer>")),
        },
        _ if spec == "wishlist" => StoreTarget::Wishlist,
        _ if spec == "cart" => StoreTarget::Cart,
        _ => StoreTarget::Home,
    };
    store_open(app.clone(), target)
        .await
        .map(|_| "opened".into())
}

/// Ids and slugs we put into Epic URLs: letters, digits, `-` and `_` only.
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn target_url(target: &StoreTarget, locale: &str) -> Result<String, UiError> {
    let lang = if safe_id(locale) { locale } else { "en-US" };
    let base = "https://store.epicgames.com";
    Ok(match target {
        StoreTarget::Home => format!("{base}/{lang}/"),
        StoreTarget::Wishlist => format!("{base}/{lang}/wishlist"),
        StoreTarget::Cart => format!("{base}/{lang}/cart"),
        StoreTarget::Product { slug } if safe_id(slug) => format!("{base}/{lang}/p/{slug}"),
        StoreTarget::Checkout {
            namespace,
            offer_id,
        } if safe_id(namespace) && safe_id(offer_id) => {
            format!("{base}/purchase?offers=1-{namespace}-{offer_id}")
        }
        StoreTarget::Search { query } => format!(
            "{base}/{lang}/browse?q={}",
            crate::auth::encode(query.trim())
        ),
        _ => return Err(err("invalid", "That isn't a store page.")),
    })
}

/// Open an Epic store page in the signed-in web window (wishlist, cart, checkout...).
/// Purchases always finish there, by the user; we never press anything on that page.
#[tauri::command]
#[specta::specta]
pub async fn store_open(app: AppHandle, target: StoreTarget) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let (_, locale) = region(&state);
    let url = target_url(&target, &locale)?;
    tracing::info!(page = ?std::mem::discriminant(&target), "opening Epic store page");
    crate::auth::open_signed_in(&app, &url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_urls_only_take_safe_ids() {
        let ok = target_url(
            &StoreTarget::Checkout {
                namespace: "ns1".into(),
                offer_id: "abc_2".into(),
            },
            "en-US",
        )
        .unwrap();
        assert_eq!(
            ok,
            "https://store.epicgames.com/purchase?offers=1-ns1-abc_2"
        );
        assert!(target_url(
            &StoreTarget::Product {
                slug: "../../evil".into()
            },
            "en-US"
        )
        .is_err());
        assert!(target_url(
            &StoreTarget::Product {
                slug: "x?y=1".into()
            },
            "en-US"
        )
        .is_err());
        assert_eq!(
            target_url(&StoreTarget::Wishlist, "en-US/../x").unwrap(),
            "https://store.epicgames.com/en-US/wishlist"
        );
        assert_eq!(
            target_url(
                &StoreTarget::Search {
                    query: "a b&c".into()
                },
                "de-DE"
            )
            .unwrap(),
            "https://store.epicgames.com/de-DE/browse?q=a%20b%26c"
        );
    }

    #[test]
    fn images_are_resized_and_limited_to_epic_cdns() {
        assert_eq!(
            resized(Some("https://cdn1.epicgames.com/a.png".into()), 360, 480).as_deref(),
            Some("https://cdn1.epicgames.com/a.png?resize=1&w=360&h=480&quality=medium")
        );
        assert_eq!(
            resized(Some("https://evil.example/a.png".into()), 1, 1),
            None
        );
    }

    #[test]
    fn ownership_rules() {
        let offer = |items: &[&str], kind: &str| Offer {
            id: "o".into(),
            namespace: "n".into(),
            title: "T".into(),
            seller: String::new(),
            slug: None,
            offer_type: kind.into(),
            effective_date: None,
            image_tall: None,
            image_wide: None,
            price: None,
            item_ids: items.iter().map(|s| s.to_string()).collect(),
        };
        let owned = Owned {
            items: ["a".to_owned()].into(),
            base_namespaces: ["n".to_owned()].into(),
        };
        let none = Owned::default();
        assert!(to_ui(offer(&["a"], "DLC"), &owned).owned);
        assert!(
            !to_ui(offer(&["a", "b"], "BUNDLE"), &owned).owned,
            "bundle not fully owned"
        );
        assert!(!to_ui(offer(&[], "DLC"), &owned).owned);
        assert!(
            to_ui(offer(&["x"], "BASE_GAME"), &owned).owned,
            "base game: same namespace"
        );
        assert!(
            !to_ui(offer(&["x"], "EDITION"), &owned).owned,
            "an edition isn't the base game"
        );
        assert!(!to_ui(offer(&["x"], "BASE_GAME"), &none).owned);
    }
}
