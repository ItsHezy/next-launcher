// Shared motion (ADR 0018). Short, transform and opacity only, never while the app starts.
// Settings › General › Animations: Match Windows (none when Windows' "Animation effects" are
// off), On or Off. app.css switches CSS animations and transitions off in the same cases; what
// is played from here (Web Animations, Svelte transitions) checks `moving()` itself.

import { flip, type AnimationConfig } from 'svelte/animate';
import { cubicIn, cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

const EASE_OUT = 'cubic-bezier(0.2, 0.8, 0.2, 1)';
const EASE_SPRING = 'cubic-bezier(0.2, 0.9, 0.25, 1.08)';

const less = typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null;

/** Set by main.ts once start-up is done: the first screen appears as it is, without motion. */
export const motion = { ready: false };

/** Whether to animate now. */
export function moving(): boolean {
  const mode = document.documentElement.dataset.motion;
  return motion.ready && (mode === 'on' || (mode !== 'off' && !less?.matches));
}

/** Sent when the last animation played from here ends. Epic's page (a native view placed from
 * the layout) re-measures then, as a moving parent shifts where its box is for a moment. */
export const SETTLED = 'nl-settled';
let running = 0;

function play(el: Element, frames: Keyframe[], opts: KeyframeAnimationOptions) {
  if (!moving()) return;
  const a = el.animate(frames, opts);
  running++;
  const end = () => {
    if (--running === 0) window.dispatchEvent(new Event(SETTLED));
  };
  a.finished.then(end, end);
}

/** A section or page coming into sight: fades in and settles `rise` px upward. */
export function enter(el: Element | null | undefined, rise = 8) {
  if (!el) return;
  play(el, [{ opacity: 0, transform: `translateY(${rise}px)` }, { opacity: 1, transform: 'none' }], {
    duration: 240,
    easing: EASE_OUT,
  });
}

/** A full-window layer (Settings) opening over the app, or the app coming back from under it. */
export function zoom(el: Element | null | undefined, from: number) {
  if (!el) return;
  play(el, [{ opacity: 0, transform: `scale(${from})` }, { opacity: 1, transform: 'none' }], {
    duration: 260,
    easing: EASE_OUT,
  });
}

const RISE: Keyframe[] = [
  { opacity: 0, transform: 'translateY(14px) scale(0.98)' },
  { opacity: 1, transform: 'none' },
];
const step = (i: number) => Math.min(i, 12) * 28;

/** Cards that enter together, one after another (capped, so a long list doesn't trail). */
export function stagger(els: Iterable<Element>) {
  let i = 0;
  for (const el of els) play(el, RISE, { duration: 340, delay: step(i++), easing: EASE_SPRING, fill: 'backwards' });
}

// Cards mounted in the same frame form one group.
let group = 0;
let groupFrame = 0;

/** Action: a card rises in when it's added, after the cards added just before it. */
export function rise(node: Element) {
  if (!moving()) return;
  if (!groupFrame) groupFrame = requestAnimationFrame(() => (group = groupFrame = 0));
  play(node, RISE, { duration: 340, delay: step(group++), easing: EASE_SPRING, fill: 'backwards' });
}

/** Action: a short bump when `key` changes (a game's status, a count), never when first shown. */
export function bump(node: Element, _key: unknown) {
  return {
    update() {
      play(
        node,
        [
          { opacity: 0.3, transform: 'scale(0.86)' },
          { opacity: 1, transform: 'scale(1.06)', offset: 0.6 },
          { opacity: 1, transform: 'none' },
        ],
        { duration: 340, easing: EASE_OUT },
      );
    },
  };
}

/** Action: one highlight that glides to the chosen item inside `node` (the first match of
 * `selector`), behind the items. Pass a `key` that changes whenever the choice may have moved.
 * `node` must be positioned and its items `position: relative` (so they draw above it); while
 * it shows, `node` has the class `glides`, so the chosen item can drop its own background. */
export function glide(node: HTMLElement, opts: { selector: string; key: unknown }) {
  let selector = opts.selector;
  const pill = document.createElement('span');
  pill.className = 'glide';
  pill.setAttribute('aria-hidden', 'true');
  pill.hidden = true;
  node.prepend(pill);
  let frame = 0;

  const place = () => {
    const el = node.querySelector<HTMLElement>(selector);
    if (!el) {
      pill.hidden = true;
      node.classList.remove('glides');
      return;
    }
    // Not on screen (a hidden view): placed again when it shows.
    if (el.offsetWidth === 0) return;
    // Layout offsets, so a parent's entrance (scale, translate) doesn't skew the result.
    let x = 0;
    let y = 0;
    for (let e: HTMLElement | null = el; e && e !== node; e = e.offsetParent as HTMLElement | null) {
      x += e.offsetLeft;
      y += e.offsetTop;
    }
    // Showing again after it was hidden: jump there, then glide from there on.
    const jump = pill.hidden;
    if (jump) pill.classList.remove('on');
    pill.hidden = false;
    node.classList.add('glides');
    pill.style.transform = `translate(${x}px, ${y}px)`;
    pill.style.width = `${el.offsetWidth}px`;
    pill.style.height = `${el.offsetHeight}px`;
    if (jump) {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => pill.classList.add('on'));
    }
  };

  const ro = new ResizeObserver(place);
  ro.observe(node);
  place();
  return {
    update(next: { selector: string; key: unknown }) {
      selector = next.selector;
      place();
    },
    destroy() {
      ro.disconnect();
      cancelAnimationFrame(frame);
      pill.remove();
    },
  };
}

/** Action: a panel fades in when it's added (no movement, e.g. around Epic's page). */
export function appear(node: Element) {
  play(node, [{ opacity: 0 }, { opacity: 1 }], { duration: 180, easing: EASE_OUT });
}

/** Action: `enter` when added. */
export function arrive(node: Element) {
  enter(node);
}

type Dir = { direction: 'in' | 'out' | 'both' };

/** What's leaving no longer takes clicks. */
function leaving(node: Element, opts?: Dir) {
  const out = opts?.direction === 'out';
  if (out && node instanceof HTMLElement) node.style.pointerEvents = 'none';
  return out;
}

/** Transition: dialog backdrops. */
export function fade(node: Element, _?: unknown, opts?: Dir): TransitionConfig {
  const out = leaving(node, opts);
  return { duration: moving() ? (out ? 160 : 200) : 0, easing: cubicOut, css: (t) => `opacity: ${t}` };
}

/** Transition: dialogs. */
export function pop(node: Element, _?: unknown, opts?: Dir): TransitionConfig {
  const out = leaving(node, opts);
  return {
    duration: moving() ? (out ? 160 : 220) : 0,
    easing: out ? cubicIn : cubicOut,
    css: (t) => `opacity: ${t}; transform: scale(${0.96 + 0.04 * t})`,
  };
}

/** Transition: menus and popovers, toward the button that opened them. */
export function drop(node: Element, _?: unknown, opts?: Dir): TransitionConfig {
  const out = leaving(node, opts);
  return {
    duration: moving() ? (out ? 120 : 200) : 0,
    easing: out ? cubicIn : cubicOut,
    css: (t) => `opacity: ${t}; transform: translateY(${(t - 1) * 4}px) scale(${0.97 + 0.03 * t})`,
  };
}

/** Transition: list items and cards that come and go (toasts, Quick Launch, the download). */
export function slide(node: Element, { y = 8, x = 0 }: { y?: number; x?: number } = {}, opts?: Dir): TransitionConfig {
  const out = leaving(node, opts);
  return {
    duration: moving() ? (out ? 180 : 260) : 0,
    easing: out ? cubicIn : cubicOut,
    css: (t) => `opacity: ${t}; transform: translate(${(1 - t) * x}px, ${(1 - t) * y}px) scale(${0.96 + 0.04 * t})`,
  };
}

/** Animation: the rest of a list moving into place when an item comes or goes. */
export function shift(node: Element, rects: { from: DOMRect; to: DOMRect }): AnimationConfig {
  return flip(node, rects, { duration: moving() ? 240 : 0, easing: cubicOut });
}
