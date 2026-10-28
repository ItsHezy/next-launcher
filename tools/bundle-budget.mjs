// Fails if the UI bundle grows past the brief's budget (section 3):
//   initial route ≤ 80 KB gzipped JS, all JS ≤ 250 KB gzipped.
// Usage: node tools/bundle-budget.mjs [distDir]   (run after `npm -w ui run build`)
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';

const dist = process.argv[2] ?? 'ui/dist';
const INITIAL_MAX = 80 * 1024;
const TOTAL_MAX = 250 * 1024;

const gz = (file) => gzipSync(readFileSync(join(dist, file)), { level: 9 }).length;
const kb = (n) => `${(n / 1024).toFixed(1)} KB`;

const html = readFileSync(join(dist, 'index.html'), 'utf8');
// Entry script plus statically preloaded chunks = what the first paint needs.
const initial = new Set(
  [...html.matchAll(/(?:src|href)="\/?(assets\/[^"]+\.js)"/g)].map((m) => m[1]),
);
const allJs = readdirSync(join(dist, 'assets'))
  .filter((f) => f.endsWith('.js'))
  .map((f) => `assets/${f}`);

const initialSize = [...initial].reduce((s, f) => s + gz(f), 0);
const totalSize = allJs.reduce((s, f) => s + gz(f), 0);

console.log(`initial route JS: ${kb(initialSize)} gz (budget ${kb(INITIAL_MAX)}) [${[...initial].join(', ')}]`);
console.log(`total JS:         ${kb(totalSize)} gz (budget ${kb(TOTAL_MAX)}) across ${allJs.length} chunks`);

let failed = false;
if (initial.size === 0) {
  console.error('could not find the entry script in index.html');
  failed = true;
}
if (initialSize > INITIAL_MAX) {
  console.error(`FAIL: initial route is ${kb(initialSize - INITIAL_MAX)} over budget`);
  failed = true;
}
if (totalSize > TOTAL_MAX) {
  console.error(`FAIL: total JS is ${kb(totalSize - TOTAL_MAX)} over budget`);
  failed = true;
}
process.exit(failed ? 1 : 0);
