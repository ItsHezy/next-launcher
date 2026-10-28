// Startup benchmark: runs the release binary with --bench-startup N times and
// reports min / median / max for the key marks.
//
//   node tools/bench-startup.mjs [runs=7] [exe=target/release/next-launcher.exe] [--mock-games N]
//
// "cold" = fresh data dir (new SQLite file + new WebView2 profile) every run.
// "warm" = the same data dir reused (what users see on every normal start).
// True cold-boot numbers (empty OS file cache) need a reboot and are recorded by hand in docs/perf.md.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const args = process.argv.slice(2);
const runs = Number(args.find((a) => /^\d+$/.test(a)) ?? 7);
const exe = resolve(args.find((a) => a.endsWith('.exe')) ?? 'target/release/next-launcher.exe');
const mockIdx = args.indexOf('--mock-games');
const mock = mockIdx >= 0 ? ['--mock-games', args[mockIdx + 1]] : [];

// WebView2 helpers release the profile a moment after exit: retry, then give up quietly.
function clean(dir) {
  try {
    rmSync(dir, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
  } catch {
    /* best effort: temp dir */
  }
}

const KEYS = ['snapshot_read', 'window_built', 'ui:mounted', 'ui:laid_out', 'window_shown (first paint)'];

function runOnce(dataDir) {
  execFileSync(exe, ['--bench-startup', '--data-dir', dataDir, ...mock], { stdio: 'ignore', timeout: 30000 });
  const marks = JSON.parse(readFileSync(join(dataDir, 'logs', 'startup.json'), 'utf8'));
  return Object.fromEntries(marks.map((m) => [m.name, m.ms]));
}

function stats(rows, key) {
  const v = rows.map((r) => r[key]).filter((x) => typeof x === 'number').sort((a, b) => a - b);
  if (!v.length) return '—';
  const med = v[Math.floor(v.length / 2)];
  return `min ${v[0].toFixed(0)}  med ${med.toFixed(0)}  max ${v[v.length - 1].toFixed(0)}`;
}

function report(label, rows) {
  console.log(`\n${label} (${rows.length} runs, ms since main)`);
  for (const k of KEYS) console.log(`  ${k.padEnd(28)} ${stats(rows, k)}`);
}

const cold = [];
for (let i = 0; i < runs; i++) {
  const dir = mkdtempSync(join(tmpdir(), 'nl-bench-'));
  try {
    cold.push(runOnce(dir));
  } finally {
    clean(dir);
  }
}

const warmDir = join(tmpdir(), 'nl-bench-warm');
clean(warmDir);
mkdirSync(warmDir, { recursive: true });
runOnce(warmDir); // prime
const warm = [];
for (let i = 0; i < runs; i++) warm.push(runOnce(warmDir));
clean(warmDir);

report('cold (fresh profile)', cold);
report('warm (existing profile)', warm);
