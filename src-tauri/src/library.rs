//! Library sync (Phase 3): owned items + builds + catalog metadata + playtime into
//! SQLite, then cover art. Spec: docs/protocol/library.md, catalog-and-images.md.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use egs_api::library::{self, CatalogItem, CATALOG_BATCH};
use egs_api::{images, Client, Secret};
use egs_cache::{CatalogFields, GameUpsert};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::sync::Semaphore;

use crate::auth::AuthState;
use crate::commands::{AppState, UiError};

/// Re-sync while the app is open (docs/protocol/README.md: never faster than EGL).
const SYNC_EVERY: Duration = Duration::from_secs(15 * 60);
/// Re-fetch catalog metadata older than this even if the build didn't change.
const CATALOG_MAX_AGE_SECS: i64 = 7 * 24 * 3600;
const CATALOG_CONCURRENCY: usize = 6;
const IMAGE_CONCURRENCY: usize = 8;

/// Image preference per slot (docs/protocol/catalog-and-images.md).
const TALL: &[&str] = &[
    "DieselGameBoxTall",
    "OfferImageTall",
    "DieselStoreFrontTall",
    "Thumbnail",
];
const WIDE: &[&str] = &[
    "DieselGameBox",
    "OfferImageWide",
    "DieselStoreFrontWide",
    "featuredMedia",
];

/// Something in the library changed; the UI re-reads the snapshot and merges it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChanged {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub syncing: bool,
    /// Friendly message when the last sync failed.
    pub error: Option<String>,
    /// Unix seconds of the last successful sync.
    pub last_sync: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct SyncStatusChanged(pub SyncStatus);

#[derive(Default)]
pub struct LibraryState {
    running: AtomicBool,
    again: AtomicBool,
    loop_started: AtomicBool,
    images_running: AtomicBool,
    status: std::sync::Mutex<Option<SyncStatus>>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `2018-08-21T17:09:17.522Z` → unix seconds (UTC). Lenient: fractional part optional.
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

fn set_status(app: &AppHandle, status: SyncStatus) {
    if let Some(lib) = app.try_state::<LibraryState>() {
        if let Ok(mut s) = lib.status.lock() {
            *s = Some(status.clone());
        }
    }
    let _ = SyncStatusChanged(status).emit(app);
}

fn changed(app: &AppHandle, reason: &str) {
    let _ = LibraryChanged {
        reason: reason.to_owned(),
    }
    .emit(app);
}

/// Request a sync now (coalesced if one is running). Also starts the 15-minute loop once.
pub fn sync_soon(app: &AppHandle) {
    let Some(lib) = app.try_state::<LibraryState>() else {
        return;
    };
    if !lib.loop_started.swap(true, Ordering::SeqCst) {
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(SYNC_EVERY).await;
                run(app2.clone()).await;
            }
        });
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run(app).await });
}

async fn run(app: AppHandle) {
    let Some(lib) = app.try_state::<LibraryState>() else {
        return;
    };
    if lib.running.swap(true, Ordering::SeqCst) {
        lib.again.store(true, Ordering::SeqCst);
        return;
    }
    loop {
        let last = lib
            .status
            .lock()
            .ok()
            .and_then(|s| s.as_ref().and_then(|s| s.last_sync));
        set_status(
            &app,
            SyncStatus {
                syncing: true,
                error: None,
                last_sync: last,
            },
        );
        let result = sync_once(&app).await;
        match result {
            Ok(()) => set_status(
                &app,
                SyncStatus {
                    syncing: false,
                    error: None,
                    last_sync: Some(now_unix() as f64),
                },
            ),
            Err(e) => {
                tracing::warn!(error = %e.message, "library sync failed");
                set_status(
                    &app,
                    SyncStatus {
                        syncing: false,
                        error: Some(e.message),
                        last_sync: last,
                    },
                );
            }
        }
        if !lib.again.swap(false, Ordering::SeqCst) {
            break;
        }
    }
    lib.running.store(false, Ordering::SeqCst);
    kick_images(&app);
}

fn catalog_fields(item: &CatalogItem) -> CatalogFields {
    CatalogFields {
        title: item.title.clone(),
        developer: item.developer.clone(),
        is_dlc: item.is_dlc() || item.has_category("addons"),
        artifact_id: item.release_info.iter().find_map(|r| r.app_id.clone()),
        third_party: item
            .custom_attributes
            .get("ThirdPartyManagedApp")
            .or_else(|| item.custom_attributes.get("ThirdPartyManagedProvider"))
            .filter(|v| !v.is_empty())
            .cloned(),
        main_game: item.main_game_item.clone(),
        meta_json: item.raw.to_string(),
        tall_url: item.image(TALL).map(|k| k.url.clone()),
        wide_url: item.image(WIDE).map(|k| k.url.clone()),
    }
}

/// Fetch catalog items for `(namespace, id)` pairs, batched per namespace, ≤ 6 in flight.
async fn fetch_catalog(
    api: &Client,
    access: &Secret,
    wanted: Vec<(String, String)>,
    country: &str,
    locale: &str,
) -> HashMap<String, CatalogItem> {
    let mut by_ns: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (ns, id) in wanted {
        by_ns.entry(ns).or_default().push(id);
    }
    let sem = Arc::new(Semaphore::new(CATALOG_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for (ns, ids) in by_ns {
        for chunk in ids.chunks(CATALOG_BATCH) {
            let (api, access, sem) = (api.clone(), access.clone(), sem.clone());
            let (ns, chunk, country, locale) = (
                ns.clone(),
                chunk.to_vec(),
                country.to_owned(),
                locale.to_owned(),
            );
            tasks.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let refs: Vec<&str> = chunk.iter().map(String::as_str).collect();
                library::catalog_items(&api, &access, &ns, &refs, &country, &locale).await
            });
        }
    }
    let mut out = HashMap::new();
    let mut failures = 0usize;
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(map)) => out.extend(map),
            Ok(Err(e)) => {
                failures += 1;
                tracing::debug!(error = %e, "catalog batch failed");
            }
            Err(_) => failures += 1,
        }
    }
    if failures > 0 {
        tracing::warn!(
            failures,
            "some catalog batches failed; those games keep cached metadata"
        );
    }
    out
}

async fn sync_once(app: &AppHandle) -> Result<(), UiError> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| UiError::internal("app state"))?;
    let auth = app
        .try_state::<AuthState>()
        .ok_or_else(|| UiError::internal("auth state"))?;
    let session = auth.session.clone();
    let Some(account) = session.current().await else {
        return Ok(()); // signed out: keep the cached library
    };
    let owner = account.account_id.as_str();
    let access = session.access_token().await?;
    let api = session.api().clone();
    let country = state
        .cache
        .snapshot()
        .ok()
        .and_then(|s| s.account)
        .and_then(|a| a.country)
        .unwrap_or_else(|| "US".into());
    let locale = "en-US";
    let started = std::time::Instant::now();

    let assets = library::assets(&api, &access, "Windows").await?;
    let builds: HashMap<String, String> = assets
        .iter()
        .filter(|a| a.namespace != "ue")
        .map(|a| (a.app_name.clone(), a.build_version.clone()))
        .collect();

    // Owned list unchanged since last time? Then only build versions can differ.
    if let Some(token) = state.cache.library_token(owner)? {
        if library::state_token_valid(&api, &access, &token)
            .await
            .unwrap_or(false)
        {
            let versions: Vec<(String, String)> = builds.into_iter().collect();
            let updated = state.cache.update_versions(owner, &versions)?;
            let played = sync_playtime(&api, &access, owner, &state).await;
            state.cache.set_sync_state(owner, None, false)?;
            tracing::info!(
                updated = updated.len(),
                ms = started.elapsed().as_millis() as u64,
                "library unchanged (state token)"
            );
            if !updated.is_empty() || played {
                changed(app, "sync");
            }
            return Ok(());
        }
    }

    let (records, token) = library::library_items(&api, &access).await?;
    let known = state.cache.known_games(owner)?;
    let now = now_unix();

    let apps: Vec<_> = records
        .iter()
        .filter(|r| r.namespace != "ue")
        .filter(|r| r.record_type.as_deref().is_none_or(|t| t == "APPLICATION"))
        .filter_map(|r| r.app_name.as_ref().map(|a| (a.clone(), r)))
        .collect();

    let wanted: Vec<(String, String)> = apps
        .iter()
        .filter(|(app_name, _)| match known.get(app_name) {
            None => true,
            Some(k) => {
                k.latest_version != builds.get(app_name).cloned()
                    || k.catalog_at.is_none_or(|t| now - t > CATALOG_MAX_AGE_SECS)
            }
        })
        .map(|(_, r)| (r.namespace.clone(), r.catalog_item_id.clone()))
        .collect();
    let catalog = fetch_catalog(&api, &access, wanted.clone(), &country, locale).await;

    // Phase 3 verification: which key image types real items carry (types only, no titles).
    let mut image_types: BTreeMap<&str, usize> = BTreeMap::new();
    for item in catalog.values() {
        for k in &item.key_images {
            *image_types.entry(k.kind.as_str()).or_default() += 1;
        }
    }

    let upserts: Vec<GameUpsert> = apps
        .iter()
        .map(|(app_name, r)| GameUpsert {
            app_name: app_name.clone(),
            namespace: r.namespace.clone(),
            catalog_item_id: r.catalog_item_id.clone(),
            latest_version: builds.get(app_name).cloned(),
            record_type: r.record_type.clone(),
            acquired_at: r.acquisition_date.as_deref().and_then(parse_iso8601),
            fallback_title: r.sandbox_name.clone().unwrap_or_else(|| app_name.clone()),
            catalog: catalog.get(&r.catalog_item_id).map(catalog_fields),
        })
        .collect();

    let diff = state.cache.apply_library(owner, &upserts, true)?;
    let played = sync_playtime(&api, &access, owner, &state).await;
    state.cache.set_sync_state(owner, token.as_deref(), true)?;

    tracing::info!(
        records = records.len(),
        apps = apps.len(),
        installable = builds.len(),
        catalog_fetched = catalog.len(),
        catalog_wanted = wanted.len(),
        added = diff.added.len(),
        changed = diff.changed.len(),
        removed = diff.removed.len(),
        removed_mock = diff.removed_mock,
        ms = started.elapsed().as_millis() as u64,
        image_types = ?image_types,
        "library synced"
    );
    if !diff.is_empty() || played {
        changed(app, "sync");
    }
    Ok(())
}

/// Returns true if any playtime changed.
async fn sync_playtime(api: &Client, access: &Secret, owner: &str, state: &AppState) -> bool {
    match library::playtime_all(api, access, owner).await {
        Ok(list) => {
            let totals: Vec<(String, u64)> = list
                .into_iter()
                .map(|p| (p.artifact_id, p.total_time))
                .collect();
            state
                .cache
                .set_playtime(owner, &totals)
                .map(|c| !c.is_empty())
                .unwrap_or(false)
        }
        Err(e) => {
            tracing::debug!(error = %e, "playtime unavailable");
            false
        }
    }
}

// ---- cover art -----------------------------------------------------------------

fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn ext_for(content_type: &str) -> &'static str {
    match content_type {
        "image/webp" => "webp",
        "image/png" => "png",
        "image/avif" => "avif",
        _ => "jpg",
    }
}

/// Write via a temp file + rename so a crash never leaves a half image.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Start the image worker if it isn't running.
pub fn kick_images(app: &AppHandle) {
    let Some(lib) = app.try_state::<LibraryState>() else {
        return;
    };
    if lib.images_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        images_worker(&app).await;
        if let Some(lib) = app.try_state::<LibraryState>() {
            lib.images_running.store(false, Ordering::SeqCst);
        }
    });
}

async fn images_worker(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(auth) = app.try_state::<AuthState>() else {
        return;
    };
    let api = auth.session.api().clone();
    let dir = state.paths.images.clone();
    let sem = Arc::new(Semaphore::new(IMAGE_CONCURRENCY));
    let (mut ok, mut failed) = (0usize, 0usize);

    loop {
        let batch = match state.cache.pending_images(32) {
            Ok(b) if !b.is_empty() => b,
            _ => break,
        };
        let mut tasks = tokio::task::JoinSet::new();
        for img in batch {
            let (api, sem, dir) = (api.clone(), sem.clone(), dir.clone());
            tasks.spawn(async move {
                let _permit = sem.acquire_owned().await;
                // 2× the CSS slot size for HiDPI (catalog-and-images.md).
                let (w, h) = if img.kind == "wide" {
                    (640, Some(360))
                } else {
                    (300, Some(400))
                };
                let res = images::fetch_resized(&api, &img.url, w, h).await;
                (img, res, dir)
            });
        }
        let mut saved_any = false;
        while let Some(joined) = tasks.join_next().await {
            let Ok((img, res, dir)) = joined else {
                continue;
            };
            match res {
                Ok(image) => {
                    let name = format!(
                        "{:016x}-{}.{}",
                        fnv64(&format!("{}|{}", img.app_name, img.url)),
                        img.kind,
                        ext_for(&image.content_type)
                    );
                    let path = dir.join(name);
                    match write_atomic(&path, &image.bytes) {
                        Ok(()) => {
                            let p = path.to_string_lossy();
                            if state
                                .cache
                                .image_saved(&img, &p, &image.content_type)
                                .unwrap_or(false)
                            {
                                ok += 1;
                                saved_any = true;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "could not write cover image");
                            failed += 1;
                            let _ = state.cache.image_failed(&img);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, kind = %img.kind, "cover download failed");
                    failed += 1;
                    let _ = state.cache.image_failed(&img);
                }
            }
        }
        if saved_any {
            changed(app, "images");
        }
    }
    if ok + failed > 0 {
        tracing::info!(ok, failed, "cover art downloaded");
    }
}

// ---- commands --------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn refresh_library(app: AppHandle, auth: State<'_, AuthState>) -> Result<(), UiError> {
    let signed_in = matches!(
        auth.status_snapshot(),
        crate::auth::AuthStatus::SignedIn { .. }
    );
    if !signed_in {
        return Err(UiError {
            code: "notSignedIn".into(),
            message: "Sign in to refresh your library.".into(),
        });
    }
    sync_soon(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn sync_status(lib: State<'_, LibraryState>) -> SyncStatus {
    lib.status
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or(SyncStatus {
            syncing: false,
            error: None,
            last_sync: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_dates() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso8601("2018-08-21T17:09:17.522Z"),
            Some(1_534_871_357)
        );
        assert_eq!(parse_iso8601("2000-02-29T12:00:00.000Z"), Some(951_825_600));
        assert_eq!(parse_iso8601("not a date"), None);
        assert_eq!(parse_iso8601("2018-13-01T00:00:00Z"), None);
    }

    #[test]
    fn image_names_are_stable() {
        assert_eq!(fnv64("a"), fnv64("a"));
        assert_ne!(fnv64("a|u1"), fnv64("a|u2"));
        assert_eq!(ext_for("image/webp"), "webp");
        assert_eq!(ext_for("image/jpeg"), "jpg");
    }
}
