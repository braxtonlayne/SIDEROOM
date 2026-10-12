import { observeElementRect, type Rect, type Virtualizer } from "@tanstack/react-virtual";

/**
 * Like TanStack's default rect observer, but reports `fallback` while the scroll element has no layout
 * (jsdom, display:none, first paint) so the first render still contains rows.
 */
export function observeRectWithFallback(fallback: Rect) {
  return <T extends Element>(instance: Virtualizer<T, Element>, cb: (rect: Rect) => void) =>
    observeElementRect(instance, (rect) => cb(rect.height === 0 ? fallback : rect));
}
