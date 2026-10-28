performance.mark('script');

import interLatin from './assets/fonts/inter-latin-wght-normal.woff2?url';

// Load the bundled font before the first layout, so layout never resolves
// system fallback fonts and text never renders invisible (font-display: block).
const inter = new FontFace('Inter', `url(${interLatin}) format('woff2')`, {
  weight: '100 900',
  display: 'block',
  unicodeRange:
    'U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD',
});
document.fonts.add(inter);
const fontLoaded = inter.load().then(
  () => performance.mark('font'),
  () => performance.mark('font_failed'),
);

import { mount } from 'svelte';
import './styles/app.css';
import App from './App.svelte';
import { commands } from './lib/bindings';
import { boot } from './lib/boot';

async function start() {
  // Cap the wait: a broken font must never block the app (timers are throttled
  // while the window is hidden, so the cap is a safety net, not a target).
  await Promise.race([fontLoaded, new Promise((r) => setTimeout(r, 150))]);

  const target = document.getElementById('app');
  if (!target) throw new Error('missing #app');
  mount(App, { target });
  performance.mark('mounted');

  // Pay for the first style + layout now, while the window is still hidden;
  // Rust then shows the window and the compositor paints this DOM next frame.
  void document.body.offsetHeight;
  performance.mark('laid_out');

  const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined;
  const marks = [
    ...(nav
      ? [
          { name: 'response_end', t: nav.responseEnd },
          { name: 'dom_interactive', t: nav.domInteractive },
        ]
      : []),
    ...performance.getEntriesByType('mark').map((m) => ({ name: m.name, t: m.startTime })),
  ];
  await commands.appReady({ now: performance.now(), marks });

  if (boot.devRoute) {
    const { applyDevRoute } = await import('./lib/dev');
    applyDevRoute(boot.devRoute);
  }

  if (boot.benchScroll) {
    const { runScrollBench } = await import('./lib/bench');
    await runScrollBench();
  }
}

void start();
