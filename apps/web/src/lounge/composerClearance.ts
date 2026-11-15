import { useCallback, useRef } from 'react';

/**
 * D-789 (SEC-AUD-295), D-800 (SEC-AUD-302): the floating music player never covers a message composer or a phone's
 * bottom navigation bar. Each mounted composer wrapper or bottom bar reports how far its top edge is from the
 * viewport's bottom edge (0 while it has no box, e.g. `display: none` under the keyboard); the document root carries
 * the largest value as `--sr-composer-clear` (CSS pixels), which `player/player.css` adds to a bottom dock's offset.
 * Several reporters (a composer, a thread pane, a bar): the largest wins. The last one to unmount removes the
 * property. No store, no context; the Lounge's composer and the two bars import this (it imports only react), so it
 * ships in the Lounge and Library chunks, not first paint.
 */
export const COMPOSER_CLEAR_VAR = '--sr-composer-clear';

const tracked = new Map<HTMLElement, number>();

function publish(): void {
  const root = document.documentElement;
  if (tracked.size === 0) {
    root.style.removeProperty(COMPOSER_CLEAR_VAR);
    return;
  }
  let max = 0;
  for (const v of tracked.values()) max = Math.max(max, v);
  root.style.setProperty(COMPOSER_CLEAR_VAR, `${Math.round(max)}px`);
}

function measure(el: HTMLElement): number {
  const rect = el.getBoundingClientRect();
  if (rect.height === 0) return 0; // no box (display: none): not the whole viewport height
  return Math.max(0, window.innerHeight - rect.top);
}

/** Start reporting one composer wrapper. Returns the function that stops it. */
export function trackComposer(el: HTMLElement): () => void {
  const update = () => {
    tracked.set(el, measure(el));
    publish();
  };
  update();
  const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
  ro?.observe(el);
  window.addEventListener('resize', update);
  return () => {
    ro?.disconnect();
    window.removeEventListener('resize', update);
    tracked.delete(el);
    publish();
  };
}

/** A callback ref for the composer wrapper (the wrapper element differs between the locked and the live composer). */
export function useComposerClearance(): (el: HTMLElement | null) => void {
  const stop = useRef<(() => void) | null>(null);
  return useCallback((el: HTMLElement | null) => {
    stop.current?.();
    stop.current = el ? trackComposer(el) : null;
  }, []);
}

/** The same hook for a bottom navigation bar (D-800), so a bar does not read as a composer. */
export const useBottomClearance = useComposerClearance;
