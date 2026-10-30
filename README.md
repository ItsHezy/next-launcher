# Next Launcher

A fast, small, independent client for the Epic Games Store on Windows, built with Rust,
Tauri 2 and Svelte 5. It signs in with your own Epic account and does what the official
launcher does for your library: install, update, verify, move and launch games, cloud saves,
the store, and your account settings.

Next Launcher is not made, endorsed or supported by Epic Games. It uses your own account only,
never works around ownership checks, and never touches anti-cheat software.

## Build

Requirements: Rust (stable), Node.js 22+, the WebView2 runtime (part of Windows 11).

```
npm install
npm run dev      # development build with hot reload
npm run build    # release build and NSIS installer
npm run check    # svelte-check + eslint
cargo test --workspace
```

More: `PROGRESS.md` (what's built), `docs/protocol/` (how Epic's services work, as used here),
`docs/decisions/` (design decisions), `docs/perf.md` (measurements).

## License

Copyright (C) 2026 the Next Launcher authors.

Next Launcher is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version. It is distributed in the hope that it
will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
or FITNESS FOR A PARTICULAR PURPOSE. See [LICENSE](LICENSE) for details.

Third-party components and their licenses are listed in `src-tauri/THIRD-PARTY-NOTICES.txt`
(regenerate with `npm run licenses`).
