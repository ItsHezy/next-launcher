# ADR 0004: Cover images, resized by Epic's CDN and stored as served

Status: **Proposed**. It adjusts a section 3 detail ("resized to WebP thumbnails"), so it needs the owner's OK.

## Observation (Phase 0 probe)

Epic's image CDN (Akamai Image Server on `cdn1.epicgames.com` and `cdn2.unrealengine.com`)
resizes on request: `?resize=1&w=<px>&h=<px>&quality=medium`. A 3:4 cover went from 489 KB to
29–45 KB. The output format is the CDN's choice (mostly JPEG, once WebP) and doesn't follow `Accept`.

## Proposal

Ask the CDN for exactly the size we display (2× for HiDPI) and store the bytes as served (JPEG or
WebP), with their content type. Serve them from disk via `img://`. No local decoding or encoding.

## Why not transcode to WebP locally

- Lossy WebP encoding needs libwebp (C, adds roughly 0.3–0.6 MB to the binary and CPU work per image).
  The pure-Rust `image-webp` encoder is lossless only, and lossless WebP of a photo is *larger* than the JPEG.
- A 300×400 JPEG at quality "medium" is already ~30 KB. A grid of 500 covers is about 15 MB on disk,
  decoded lazily by WebView2.
- The goal of section 3 (small thumbnails, fetched once, served from disk, no reflow) is fully met.

## If rejected

Add `image` (JPEG decode) + `webp` (libwebp) crates behind an `egs-cache` feature and transcode
after download. Cost: about 0.5 MB installer and some CPU on first sync.
