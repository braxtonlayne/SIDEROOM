import { useCallback, useRef } from 'react';

/**
 * D-805 (SEC-AUD-305): on a phone the end of a Cinema, Library or Arcade page scrolls clear of the floating music
 * player. While the player's dock element is shown in a bottom dock (`data-dock` starts with `bottom`) the document
 * root carries `--sr-player-clear`: the dock's distance from the viewport's bottom edge to its top edge, in CSS
 * pixels (its height plus its lift, which already includes the composer or bar clearance of D-789 and D-800).
 * `pageEnd.css` gives each room's last block that much room after it. A top dock, no element (nothing queued, the
 * player closed) or an unmount removes the property. It imports only react: it ships with the player host's chunk.
 * The Lounge is not touched (D-792).
 */
export const PLAYER_CLEAR_VAR = '--sr-player-clear';

function publish(el: HTMLElement): void {
  const root = document.documentElement;
  const dock = el.getAttribute('data-dock') ?? '';
  const rect = el.getBoundingClientRect();
  if (!dock.startsWith('bottom') || rect.height === 0) {
    if (root.style.getPropertyValue(PLAYER_CLEAR_VAR)) root.style.removeProperty(PLAYER_CLEAR_VAR);
    return;
  }
  const next = `${Math.round(Math.max(0, window.innerHeight - rect.top))}px`;
  // Only on a change: the root's own style attribute is watched below, and a write must not wake it again.
  if (root.style.getPropertyValue(PLAYER_CLEAR_VAR) !== next)
    root.style.setProperty(PLAYER_CLEAR_VAR, next);
}

/** Start publishing for one dock element. Returns the function that stops it and removes the property. */
export function trackPlayerDock(el: HTMLElement): () => void {
  const update = () => publish(el);
  update();
  const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
  ro?.observe(el);
  // The dock moves between corners by changing `data-dock` (and the lift follows a composer or bar), so watch it.
  const mo = typeof MutationObserver === 'undefined' ? null : new MutationObserver(update);
  mo?.observe(el, { attributes: true, attributeFilter: ['data-dock'] });
  // The lift follows --sr-composer-clear on the root (a bar or a composer mounting moves the dock without resizing it).
  const rootMo = typeof MutationObserver === 'undefined' ? null : new MutationObserver(update);
  rootMo?.observe(document.documentElement, { attributes: true, attributeFilter: ['style'] });
  window.addEventListener('resize', update);
  return () => {
    ro?.disconnect();
    mo?.disconnect();
    rootMo?.disconnect();
    window.removeEventListener('resize', update);
    document.documentElement.style.removeProperty(PLAYER_CLEAR_VAR);
  };
}

/** A callback ref for the player's dock element. */
export function usePlayerPageEnd(): (el: HTMLElement | null) => void {
  const stop = useRef<(() => void) | null>(null);
  return useCallback((el: HTMLElement | null) => {
    stop.current?.();
    stop.current = el ? trackPlayerDock(el) : null;
  }, []);
}
