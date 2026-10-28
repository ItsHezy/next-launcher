// Scroll benchmark (`--bench-scroll`): scrolls the library grid top → bottom one
// step per frame and records frame intervals. Rust writes the result and exits.

import { commands } from './bindings';
import { router } from './state/router.svelte';

function pct(sorted: number[], p: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))] ?? 0;
}

async function frames(n: number, step: (i: number) => void): Promise<number[]> {
  const out: number[] = [];
  let last = performance.now();
  for (let i = 0; i < n; i++) {
    await new Promise<void>((r) =>
      requestAnimationFrame((t) => {
        out.push(t - last);
        last = t;
        step(i);
        r();
      }),
    );
  }
  return out.slice(1);
}

export async function runScrollBench() {
  await new Promise((r) => setTimeout(r, 800)); // let the shown window settle
  const scroller = document.querySelector<HTMLElement>('[data-scroll="library"]');
  if (!scroller) return;

  // Baseline: idle frames give the display's refresh interval.
  const idle = await frames(60, () => {});
  const vsync = [...idle].sort((a, b) => a - b)[Math.floor(idle.length / 2)] ?? 16.7;

  const max = scroller.scrollHeight - scroller.clientHeight;
  const n = 360;
  const scroll = await frames(n, (i) => {
    scroller.scrollTop = (max * (i + 1)) / n;
  });
  const sorted = [...scroll].sort((a, b) => a - b);
  const mean = scroll.reduce((a, b) => a + b, 0) / scroll.length;

  // Tab switches (already-visited and first-visit) as measured by the router.
  for (const r of ['store', 'library', 'settings', 'library', 'store', 'library'] as const) {
    router.go(r);
    await new Promise((res) => setTimeout(res, 250));
  }

  const result = {
    games: document.querySelectorAll('[role="listitem"]').length,
    scrollPx: Math.round(max),
    vsyncMs: +vsync.toFixed(2),
    frames: scroll.length,
    meanFrameMs: +mean.toFixed(2),
    p95FrameMs: +pct(sorted, 0.95).toFixed(2),
    p99FrameMs: +pct(sorted, 0.99).toFixed(2),
    maxFrameMs: +(sorted[sorted.length - 1] ?? 0).toFixed(2),
    over1_5Vsync: scroll.filter((d) => d > vsync * 1.5).length,
    effectiveFps: +(1000 / mean).toFixed(1),
    tabSwitches: router.timings.map((t) => ({ route: t.route, firstVisit: t.firstVisit, ms: +t.ms.toFixed(1) })),
  };
  await commands.benchReport('bench-scroll', JSON.stringify(result, null, 2));
}
