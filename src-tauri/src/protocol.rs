//! `img` protocol: serves cached cover art from disk.
//!
//! URL shape: `<base>/<app_name>/<kind>` where `<base>` is `http://img.localhost`
//! on Windows (WebView2 maps custom schemes to http) and `img://localhost` elsewhere.
//! Paths are never built from the URL; the file is looked up in the cache index.

use std::sync::Arc;

use egs_cache::Cache;
use tauri::http::{header, Response, StatusCode};

pub const SCHEME: &str = "img";

pub fn base_url() -> &'static str {
    if cfg!(windows) {
        "http://img.localhost"
    } else {
        "img://localhost"
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = code;
    r
}

pub fn handle(cache: &Arc<Cache>, path: &str) -> Response<Vec<u8>> {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let (Some(app), Some(kind)) = (parts.next(), parts.next()) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let found = match cache.image_file(app, kind) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "image lookup failed");
            return status(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let Some((file, content_type)) = found else {
        return status(StatusCode::NOT_FOUND);
    };
    match std::fs::read(&file) {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}
