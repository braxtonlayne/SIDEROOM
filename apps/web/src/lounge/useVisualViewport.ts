import { useEffect } from 'react';

/** Keyboard is "open" when the visual viewport is at least this much shorter than the layout viewport. */
const KEYBOARD_MIN = 120;

/** Pure: the height to give the Lounge root, or null to leave it at 100dvh. */
export function keyboardHeight(vvHeight: number, innerHeight: number): number | null {
  return innerHeight - vvHeight >= KEYBOARD_MIN ? Math.round(vvHeight) : null;
}

/**
 * iOS Safari (and installed iOS PWAs) keep the layout viewport the same size when the keyboard opens and
 * scroll the page instead, which hides the composer behind the keyboard (XC-PWA-007). While the keyboard is
 * up we size the app to the visual viewport via `--sr-vvh` and undo the page scroll the keyboard caused.
 * Browsers that resize the layout viewport themselves (Chrome Android) report no difference, so nothing
 * changes for them.
 */
export function useVisualViewport(): void {
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return;
    const root = document.documentElement;
    const apply = () => {
      const h = keyboardHeight(vv.height, window.innerHeight);
      if (h === null) {
        root.style.removeProperty('--sr-vvh');
        return;
      }
      root.style.setProperty('--sr-vvh', `${h}px`);
      if (window.scrollY !== 0) window.scrollTo(0, 0);
    };
    apply();
    vv.addEventListener('resize', apply);
    vv.addEventListener('scroll', apply);
    return () => {
      vv.removeEventListener('resize', apply);
      vv.removeEventListener('scroll', apply);
      root.style.removeProperty('--sr-vvh');
    };
  }, []);
}
