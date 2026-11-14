import { useCallback, useRef } from 'react';

/**
 * D-789 (SEC-AUD-295): the floating music player never covers a message composer. Each mounted composer wrapper
 * reports how far its top edge is from the viewport's bottom edge; the document root carries the largest value as
 * `--sr-composer-clear` (CSS pixels), which `player/player.css` adds to a bottom dock's offset. Several composers
 * (a channel and a thread pane): the largest wins. The last one to unmount removes the property. No store, no
 * context; only the Lounge's composer imports this, so it ships in the Lounge chunk, not first paint.
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
  return Math.max(0, window.innerHeight - el.getBoundingClientRect().top);
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
