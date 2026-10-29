//! Cover art from Epic's image CDN, resized server-side (docs/protocol/catalog-and-images.md).

use crate::error::ApiError;
use crate::http::{Client, Retry};

/// Larger responses are refused (a resized cover is ~15–45 KB).
const MAX_BYTES: usize = 5 * 1024 * 1024;

pub struct Image {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// `url?resize=1&w=..&h=..&quality=medium`. Only `https` sources are fetched.
pub fn resized_url(url: &str, w: u32, h: Option<u32>) -> Option<String> {
    if !url.starts_with("https://") {
        return None;
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    Some(match h {
        Some(h) => format!("{url}{sep}resize=1&w={w}&h={h}&quality=medium"),
        None => format!("{url}{sep}resize=1&w={w}&quality=medium"),
    })
}

/// Download a resized image. The format is the CDN's choice (usually JPEG).
pub async fn fetch_resized(
    client: &Client,
    url: &str,
    w: u32,
    h: Option<u32>,
) -> Result<Image, ApiError> {
    let full =
        resized_url(url, w, h).ok_or_else(|| ApiError::Invalid("image url is not https".into()))?;
    let resp = client
        .send(client.http().get(full), Retry::Idempotent)
        .await?;
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .split(';')
        .next()
        .unwrap_or("image/jpeg")
        .trim()
        .to_owned();
    if !content_type.starts_with("image/") {
        return Err(ApiError::Decode(format!("not an image: {content_type}")));
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > MAX_BYTES {
        return Err(ApiError::Decode("image too large".into()));
    }
    Ok(Image {
        bytes: bytes.to_vec(),
        content_type,
    })
}

#[cfg(test)]
mod tests {
    use super::resized_url;

    #[test]
    fn builds_resize_query() {
        assert_eq!(
            resized_url("https://cdn1.epicgames.com/x.jpg", 300, Some(400)).as_deref(),
            Some("https://cdn1.epicgames.com/x.jpg?resize=1&w=300&h=400&quality=medium")
        );
        assert_eq!(
            resized_url("https://cdn1.epicgames.com/x?a=1", 640, None).as_deref(),
            Some("https://cdn1.epicgames.com/x?a=1&resize=1&w=640&quality=medium")
        );
        assert!(resized_url("http://example.com/x.jpg", 1, None).is_none());
    }
}
