// Runs in Epic's pages inside Settings before their own scripts (ADR 0016): the app's look
// (webpane.css, inlined by webpane.rs as __CSS__) and software behaviour instead of a browser's.
// Reads nothing from the page and sends nothing anywhere.
(() => {
  if (window.__nextLauncherPane) return;
  window.__nextLauncherPane = true;
  if (location.protocol !== 'https:' || !/(^|\.)(epicgames|fortnite|unrealengine)\.com$/.test(location.hostname)) return;

  const CSS = __CSS__;
  const put = () => {
    const parent = document.head || document.documentElement;
    if (!parent) return false;
    let style = document.getElementById('next-launcher-pane');
    if (!style) {
      style = document.createElement('style');
      style.id = 'next-launcher-pane';
      style.textContent = CSS;
    }
    if (style.parentNode !== parent) parent.appendChild(style);
    return true;
  };
  if (!put()) {
    const wait = new MutationObserver(() => {
      if (put()) wait.disconnect();
    });
    wait.observe(document, { childList: true, subtree: true });
  }
  // Again once <head> exists, after the page's own early styles.
  document.addEventListener('DOMContentLoaded', put, { once: true });

  // Epic's own blue on custom controls the stylesheet can't name (e.g. a checkbox drawn by the
  // page) becomes the app's accent. Before each paint after a change, so it never flashes;
  // undone first each time, so a control that changes state gets its new colour.
  const EPIC_BLUE = 'rgb(38, 187, 255)';
  const TINT = ['background-color', 'color', 'border-color'];
  let queued = false;
  const tint = () => {
    queued = false;
    for (const el of document.querySelectorAll('[data-nl-tint]')) {
      for (const p of TINT) el.style.removeProperty(p);
      el.removeAttribute('data-nl-tint');
    }
    const changes = [];
    for (const el of document.querySelectorAll('#root [class*="Mui"], #root a, #root svg')) {
      const s = getComputedStyle(el);
      if (s.backgroundColor === EPIC_BLUE) changes.push([el, 'background-color', 'var(--nl-accent-fill)'], [el, 'color', '#fff']);
      else if (s.color === EPIC_BLUE) changes.push([el, 'color', 'var(--nl-accent-text)']);
      if (s.borderTopColor === EPIC_BLUE) changes.push([el, 'border-color', 'var(--nl-accent-text)']);
    }
    for (const [el, p, v] of changes) {
      el.style.setProperty(p, v, 'important');
      el.setAttribute('data-nl-tint', '');
    }
  };
  const schedule = () => {
    if (!queued) {
      queued = true;
      requestAnimationFrame(tint);
    }
  };
  document.addEventListener(
    'DOMContentLoaded',
    () => {
      schedule();
      new MutationObserver(schedule).observe(document.body, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ['class', 'aria-checked', 'checked'],
      });
    },
    { once: true },
  );

  // No browser menus, reload, print, find, zoom, history or new-tab gestures.
  const editable = (t) => t instanceof Element && !!t.closest('input, textarea, [contenteditable]:not([contenteditable="false"])');
  const BLOCKED = new Set(['r', 'p', 'f', 'g', 'u', 's', 'o', 'n', 't', 'j', 'h', 'd', '+', '=', '-', '0']);
  addEventListener('contextmenu', (e) => editable(e.target) || e.preventDefault(), true);
  addEventListener(
    'keydown',
    (e) => {
      const k = e.key.toLowerCase();
      if (
        k === 'f3' ||
        k === 'f5' ||
        k === 'f7' ||
        k === 'browserback' ||
        k === 'browserforward' ||
        (e.altKey && (k === 'arrowleft' || k === 'arrowright' || k === 'home')) ||
        ((e.ctrlKey || e.metaKey) && BLOCKED.has(k))
      )
        e.preventDefault();
    },
    true,
  );
  addEventListener('mouseup', (e) => (e.button === 3 || e.button === 4) && e.preventDefault(), true);
  addEventListener('auxclick', (e) => e.button === 1 && e.preventDefault(), true);
  addEventListener('wheel', (e) => e.ctrlKey && e.preventDefault(), { passive: false, capture: true });
  addEventListener('dragstart', (e) => editable(e.target) || e.preventDefault(), true);
})();
