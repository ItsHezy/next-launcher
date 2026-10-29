# Catalog metadata and images

Catalog endpoint and fields: see [library.md](library.md#catalog-metadata-title-images-custom-attributes).
This file covers which image to show where, and how images are fetched and cached.

## keyImages

Every catalog item and store offer has `keyImages: [{type, url, width?, height?, md5?}]`.

| Where in our UI | Preferred types, in order | Confidence |
|---|---|---|
| Library grid (3:4 cover) | `DieselGameBoxTall`, `OfferImageTall`, `Thumbnail`, `DieselStoreFrontTall` | [REF] Heroic/Legendary usage, `OfferImageTall`/`Thumbnail` [OBS] in store feed |
| Recently played (wide card) | `DieselGameBox`, `OfferImageWide`, `DieselStoreFrontWide` | [REF], `OfferImageWide` [OBS] |
| Quick Launch (small square-ish) | `Thumbnail`, then the tall cover | [OBS] |
| Logo overlays | `DieselGameBoxLogo` | [REF] (optional, not used in v1 design) |
| Store grid | `OfferImageTall`, `Thumbnail` | [OBS] |

Types observed in the free-games feed (2026-10-28): `GalleryImage`, `OfferImageTall`,
`OfferImageWide`, `OgImage`, `Thumbnail`, `featuredMedia`, `heroCarouselVideo`. **[OBS]**
Real library, Phase 3 (38 games) **[OBS]**: `DieselGameBoxTall` 36, `DieselGameBox` 36,
`OfferImageTall` 2, `OfferImageWide` 2, `Thumbnail` 2, `DieselGameBoxLogo` 2, `Featured` 1,
`AndroidIcon` 1. The tall/wide preference lists above covered every game; 76/76 covers downloaded.

## Image CDN — [OBS]

Hosts: `cdn1.epicgames.com`, `cdn2.unrealengine.com` (both served by **Akamai Image Server**).

Server-side resizing works on both hosts with query parameters:

```
<url>?resize=1&w=<px>&h=<px>&quality=medium      (h optional; aspect ratio preserved)
```

Measured on one 3:4 cover: original 489 KB JPEG → `w=360&h=480&quality=medium` 29–45 KB.
On a 16:9 image: `w=400` 15–22 KB, `w=800&quality=high` 85–136 KB.

- The **output format is chosen by the CDN, not by `Accept`**. Responses carry no `Vary: Accept`.
  One response was WebP, every later one JPEG. The cache must accept JPEG, WebP and PNG
  and store the `Content-Type` alongside the bytes.
- `format=webp` is **not** honoured (returned JPEG).
- `Cache-Control: public, max-age=2592000` (30 days).

## Our image pipeline (egs-cache)

1. Library sync enqueues `(game_id, type, url)` for each needed image (table `game_images`).
2. A background worker (≤ 8 concurrent, started only after first paint) requests
   `url?resize=1&w=W&h=H&quality=medium` with W×H = 2× the CSS size of the slot
   (cover 300×400, wide 640×360, quick-launch 96×96 cropped by CSS). Then it writes
   `images/<sha1(url)>-<W>x<H>.<ext>` atomically (temp file + rename) and stores
   `local_path`, `etag`, `content_type`.
3. The UI loads covers from `img://localhost/<game_id>/<type>`. The custom protocol handler
   reads the file and returns it with the stored content type and `Cache-Control: max-age=31536000, immutable`.
   If missing, it returns 404 quickly. The tile shows its skeleton color and swaps in the image
   when `library://image-ready` fires.
4. Revalidation: only when the catalog item's `keyImages[].md5` or `url` changes (new
   url ⇒ new file name). We never refetch images on a timer.

No local transcoding. See [ADR 0004](../decisions/0004-image-pipeline.md) (status: *Proposed*, needs the owner's OK
because it adjusts a section 3 detail).
