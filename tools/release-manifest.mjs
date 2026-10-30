// Update manifest (`latest.json`) for a release (Phase 9, ADR 0014).
//
//   node tools/release-manifest.mjs --installer <setup.exe> --url <download URL> [--notes <file>] [--out latest.json]
//
// The installer's signature comes from `<setup.exe>.sig`, written by `tauri build` when the
// updater signing key is set (TAURI_SIGNING_PRIVATE_KEY). The version is the app's own
// (src-tauri/tauri.conf.json). The app reads this file from the newest GitHub release.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';

const args = new Map();
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i].replace(/^--/, ''), process.argv[i + 1]);

const installer = args.get('installer');
const url = args.get('url');
if (!installer || !url) {
  console.error('usage: release-manifest.mjs --installer <setup.exe> --url <download URL> [--notes <file>] [--out <file>]');
  process.exit(2);
}
if (!existsSync(`${installer}.sig`)) {
  console.error(`missing ${installer}.sig: build with TAURI_SIGNING_PRIVATE_KEY set`);
  process.exit(1);
}

const conf = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
const notes = args.get('notes') ? readFileSync(args.get('notes'), 'utf8').trim() : undefined;
const manifest = {
  version: conf.version,
  notes,
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
  platforms: {
    'windows-x86_64': {
      signature: readFileSync(`${installer}.sig`, 'utf8').trim(),
      url,
    },
  },
};

const out = args.get('out') ?? 'latest.json';
writeFileSync(out, JSON.stringify(manifest, null, 2) + '\n');
console.log(`${out}: version ${manifest.version}`);
