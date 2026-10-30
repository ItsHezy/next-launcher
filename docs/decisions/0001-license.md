# ADR 0001: Project license

Status: **Accepted** (2026-10-30), owner's decision: **GPL-3.0-or-later**.

## Context

The protocol specs were written after reading Legendary and its Heroic fork (both GPL-3.0)
and an unlicensed community documentation repo. Facts about a protocol aren't copyrightable,
and our code is written from our own specs, not translated. The brief offered GPL-3.0 or MIT.

## Decision

GPL-3.0-or-later for all of Next Launcher (every crate and the UI).

- `LICENSE` holds the verbatim GPL-3.0 text; `Cargo.toml` (`license.workspace`) and both
  `package.json` files say `GPL-3.0-or-later`.
- The installer ships `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt`; Settings → About shows the
  license and opens both.
- Dependencies are compatible: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, MPL-2.0 (5 crates,
  unmodified) and the Inter font under OFL-1.1 (`npm run licenses`). No GPL-incompatible components.

## Consequences

- Anyone who receives an installer must be able to get the matching source (GPL-3.0 §6). When
  releases are published, the source goes with them (for example the tagged repository next to
  the installer on the same release page). Open: where releases are hosted (owner, Phase 9).
- Code from Legendary/Heroic (GPL-3.0) could now be reused with attribution; we keep writing
  from our own specs anyway, so this is a safety net, not a practice.
- Others can't fold our crates into closed-source software.
