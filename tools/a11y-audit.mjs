// Accessibility audit of the running app with axe-core (dev dependency, never shipped).
//
//   1. Start the app with WebView2's debug port:  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=0
//   2. node tools/a11y-audit.mjs <ui profile>/EBWebView/DevToolsActivePort
//
// Walks the main views (library, menus, store, product page, every Settings section, a dialog)
// and prints axe violations per view. Exit code 1 if any serious or critical violation remains.
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const axeSource = readFileSync(require.resolve('axe-core/axe.min.js', { paths: ['ui'] }), 'utf8');
const port = readFileSync(process.argv[2], 'utf8').split('\n')[0].trim();
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const ws = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const ev = (expression) =>
  new Promise((r) => {
    pending.set(++id, r);
    ws.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }));
  });
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (pending.has(d.id)) {
    pending.get(d.id)(d.result?.result?.value ?? d.result?.exceptionDetails?.text);
    pending.delete(d.id);
  }
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const click = (text) =>
  ev(`(() => { const el = [...document.querySelectorAll('button, a, [role=menuitem], [role=tab]')].find(e => ((e.getAttribute('aria-label') || e.textContent || '').trim()).startsWith(${JSON.stringify(text)})); if (!el) return false; el.click(); return true; })()`);
const key = (k) => ev(`document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(k)}, bubbles: true })) ?? window.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(k)} }))`);

let serious = 0;
const report = [];
async function audit(view) {
  const res = await ev(`(async () => {
    const r = await axe.run(document, { resultTypes: ['violations'], runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'] } });
    return JSON.stringify(r.violations.map(v => ({ id: v.id, impact: v.impact, help: v.help, n: v.nodes.length, sample: v.nodes.slice(0, 2).map(x => x.target.join(' ')) })));
  })()`);
  const list = JSON.parse(res || '[]');
  // Brief section 6: every clickable thing is at least 44 px tall.
  const small = JSON.parse(
    (await ev(`JSON.stringify([...document.querySelectorAll('button, a[href], [role=menuitem], [role=tab], select, input[type=text], input[type=search]')]
      .filter(e => e.offsetParent !== null && !e.closest('[hidden]'))
      .map(e => {
        // A transparent ::before with negative insets widens the hit area (e.g. switches).
        const b = getComputedStyle(e, '::before');
        const grow = b.content !== 'none' && b.position === 'absolute' ? -(parseFloat(b.top) || 0) - (parseFloat(b.bottom) || 0) : 0;
        const r = e.getBoundingClientRect();
        return { e, r: { width: r.width, height: r.height + Math.max(0, grow) } };
      })
      .filter(({ r }) => r.width > 0 && r.height > 0 && r.height < 43.5)
      .map(({ e, r }) => (e.getAttribute('aria-label') || e.textContent || e.tagName).trim().slice(0, 40) + ' (' + Math.round(r.height) + 'px)'))`)) || '[]',
  );
  if (small.length) list.push({ id: 'hit-target-44px', impact: 'serious', help: 'Clickable elements must be at least 44 px tall (brief section 6)', n: small.length, sample: small.slice(0, 4) });
  for (const v of list) if (v.impact === 'serious' || v.impact === 'critical') serious += 1;
  report.push({ view, list });
}

ws.onopen = async () => {
  await ev(axeSource + ';true');
  await audit('library');
  if (await click('Profile menu')) { await sleep(300); await audit('profile menu'); await key('Escape'); await sleep(200); }
  if (await click('Wallet')) { await sleep(500); await audit('wallet popover'); await key('Escape'); await sleep(200); }
  if (await click('Store')) {
    await sleep(2500);
    await audit('store featured');
    await ev(`document.querySelector('.grid .card')?.click()`);
    await sleep(2500);
    await audit('store product');
    await click('Store');
    await sleep(300);
  }
  await click('Settings');
  await sleep(500);
  for (const s of ['Account', 'Wallet & rewards', 'General', 'Downloads & installs', 'Notifications', 'Cloud saves', 'Network', 'Manage games', 'Support', 'Legal & policies', 'About']) {
    if (await click(s)) {
      await sleep(s === 'Cloud saves' || s === 'Account' ? 2000 : 500);
      await audit(`settings: ${s}`);
    }
  }
  if ((await click('Wallet & rewards')) && (await click('Enter code'))) {
    await sleep(500);
    await audit('dialog: redeem code');
    await key('Escape');
  }
  for (const { view, list } of report) {
    console.log(`${view}: ${list.length ? '' : 'no violations'}`);
    for (const v of list) console.log(`  [${v.impact}] ${v.id} ×${v.n}: ${v.help}  e.g. ${v.sample.join(' | ')}`);
  }
  console.log(`\nserious/critical: ${serious}`);
  process.exit(serious ? 1 : 0);
};
