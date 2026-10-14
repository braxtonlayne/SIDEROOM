import { useEffect, type RefObject } from 'react';

export interface RovingOptions {
  /** CSS selector for the focusable items inside the container. */
  selector: string;
  /** Which arrow keys move between items. `both` accepts all four (grids move by row with up/down). */
  orientation?: 'vertical' | 'horizontal' | 'both';
}

/** Pure: the index to move to, or null if the key is not a navigation key. */
export function nextIndex(
  key: string,
  index: number,
  count: number,
  orientation: NonNullable<RovingOptions['orientation']>,
): number | null {
  if (count === 0) return null;
  const prev =
    (orientation !== 'horizontal' && key === 'ArrowUp') ||
    (orientation !== 'vertical' && key === 'ArrowLeft');
  const next =
    (orientation !== 'horizontal' && key === 'ArrowDown') ||
    (orientation !== 'vertical' && key === 'ArrowRight');
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  if (prev) return Math.max(0, index - 1);
  if (next) return Math.min(count - 1, index + 1);
  return null;
}

/**
 * Roving tabindex for a list of controls (XC-A11Y-001). The container becomes one Tab stop: only the active
 * item has `tabindex=0`, the rest `-1`, and the arrow keys, Home and End move focus between items. The
 * active item is the last one focused, else the one marked `aria-current` / `data-selected`, else the first.
 * Works on the DOM so it applies to existing markup and follows items as they come and go.
 */
export function useRovingTabindex(
  ref: RefObject<HTMLElement | null>,
  { selector, orientation = 'vertical' }: RovingOptions,
): void {
  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    let active: HTMLElement | null = null;
    const items = () => Array.from(root.querySelectorAll<HTMLElement>(selector));

    const sync = () => {
      const all = items();
      if (!all.length) return;
      if (!active || !all.includes(active)) {
        active =
          all.find(
            (el) => el.closest('[aria-current],[data-selected]') || el.hasAttribute('aria-current'),
          ) ?? all[0]!;
      }
      for (const el of all) el.tabIndex = el === active ? 0 : -1;
    };

    const onFocusIn = (e: FocusEvent) => {
      const el = (e.target as HTMLElement | null)?.closest<HTMLElement>(selector);
      if (el && root.contains(el)) {
        active = el;
        sync();
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
      const el = (e.target as HTMLElement | null)?.closest<HTMLElement>(selector);
      if (!el || el !== e.target) return;
      const all = items();
      const to = nextIndex(e.key, all.indexOf(el), all.length, orientation);
      if (to === null) return;
      e.preventDefault();
      const target = all[to];
      if (target && target !== el) {
        active = target;
        sync();
        target.focus();
      }
    };

    let raf = 0;
    const schedule = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(sync);
    };
    const mo = new MutationObserver(schedule);
    mo.observe(root, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['aria-current', 'data-selected'],
    });
    root.addEventListener('focusin', onFocusIn);
    root.addEventListener('keydown', onKeyDown);
    sync();
    return () => {
      cancelAnimationFrame(raf);
      mo.disconnect();
      root.removeEventListener('focusin', onFocusIn);
      root.removeEventListener('keydown', onKeyDown);
    };
  }, [ref, selector, orientation]);
}
