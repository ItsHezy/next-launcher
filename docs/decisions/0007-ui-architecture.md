# ADR 0007: UI architecture

Status: Accepted (2026-10-28)

- **Svelte 5 runes, no component library, no CSS framework.** One global stylesheet with the
  section 6 tokens. Components are scoped CSS. Icons are inline SVG paths drawn for this project.
- **Hand-written router** (`ui/src/lib/state/router.svelte.ts`, ~60 lines) instead of
  `svelte-spa-router`: we need "keep mounted once visited", idle prefetch and switch timing, which
  a URL router doesn't give us for free. The app has no URLs worth deep-linking yet.
- **Library is in the entry chunk.** Store, Unreal Engine, Fab and Settings are dynamic imports,
  prefetched with `requestIdleCallback` after first paint, and kept mounted (`hidden`) after their
  first visit, so switching back only toggles visibility.
- **Settings is schema-driven** (`routes/settings/schema.ts`): 11 sections of typed rows rendered by
  one row component. Search filters the schema. Sections that need more (profile card, Manage
  games, per-game cloud saves, diagnostics) add a custom block.
- **Typed IPC**: `tauri-specta` generates `ui/src/lib/bindings.ts` from the Rust command signatures
  (`cargo test -p next-launcher export_bindings`). CI fails if it is stale. Settings cross IPC as a
  `bool | number | string` union, not `serde_json::Value` (its `i64` can't be typed safely in TS).
- **Library state** uses `$state.raw` arrays replaced on change, so only tiles whose row object
  changed re-render (Phase 3's diffing sync relies on this).
- **Virtualised grid**: only visible rows + 2 overscan rows are in the DOM (25 tiles for 600 games).
- **Accessibility**: every clickable element is a `<button>`/`<a>` with a visible focus ring and
  a ≥ 44 px hit area; menus support arrow keys, Home/End and Escape; motion is disabled under
  `prefers-reduced-motion`; contrast rules are in `docs/perf.md`.

Interpretations of the brief (flag if wrong):
- "Settings → General → hide library" is implemented as a **Hidden games** list.
- "Open on" offers Library, Unreal Engine and Fab, but **not Store**, honouring "never opens on Store".
- Settings is a full-window page (sidebar and top bar hidden) with its own back link and window controls.
- Epic-account privacy has no API (Phase 0), so Account → Privacy links to Epic's privacy page
  instead of showing toggles that would only change a local value.
