// Small DOM actions shared by menus and popovers.

/** Close on pointer-down outside `node` or on Escape; focus returns to `trigger`. */
export function dismiss(node: HTMLElement, opts: { onclose: () => void; trigger?: () => HTMLElement | undefined }) {
  const down = (e: PointerEvent) => {
    if (!node.contains(e.target as Node)) opts.onclose();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      opts.onclose();
      opts.trigger?.()?.focus();
    }
  };
  document.addEventListener('pointerdown', down, true);
  document.addEventListener('keydown', key, true);
  return {
    destroy() {
      document.removeEventListener('pointerdown', down, true);
      document.removeEventListener('keydown', key, true);
    },
  };
}

/** Roving focus for role="menu": arrows/Home/End move between menuitems; first item focused on mount. */
export function menuKeys(node: HTMLElement) {
  const items = () => [...node.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])')];
  queueMicrotask(() => items()[0]?.focus());
  const key = (e: KeyboardEvent) => {
    const list = items();
    const i = list.indexOf(document.activeElement as HTMLElement);
    let n = -1;
    if (e.key === 'ArrowDown') n = (i + 1) % list.length;
    else if (e.key === 'ArrowUp') n = (i - 1 + list.length) % list.length;
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = list.length - 1;
    if (n < 0) return;
    e.preventDefault();
    list[n]?.focus();
  };
  node.addEventListener('keydown', key);
  return { destroy: () => node.removeEventListener('keydown', key) };
}
