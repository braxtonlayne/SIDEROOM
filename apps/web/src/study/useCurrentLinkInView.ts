import { useEffect, type RefObject } from 'react';
import { scrollBarTo } from './scrollBarTo';

/**
 * D-821: bring the current link of a sideways-scrolling bar into view.
 * Finds `selector` inside `ref`, and moves only the bar's own `scrollLeft` (through `scrollBarTo`, by the difference of
 * the two boxes, so it holds in a right-to-left bar too). Never `scrollIntoView`: that moves every scrolling ancestor.
 * Runs on mount, when `key` (the route) or `selector` changes, and once more after the web fonts have set the final width.
 */
export function useCurrentLinkInView(
  ref: RefObject<HTMLElement | null>,
  selector: string,
  key: unknown,
): void {
  useEffect(() => {
    let live = true;
    const show = () => {
      const bar = ref.current;
      const link = bar?.querySelector<HTMLElement>(selector);
      if (bar && link) scrollBarTo(bar, link);
    };
    show();
    void document.fonts?.ready.then(() => live && show());
    return () => {
      live = false;
    };
  }, [ref, selector, key]);
}
