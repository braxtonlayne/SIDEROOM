import { useEffect, type RefObject } from 'react';

export const SWIPE_MIN_PX = 90;
export const SWIPE_MAX_DRIFT_PX = 60;

/** True when a touch moved far enough down and stayed mostly vertical. */
export const isSwipeDown = (dx: number, dy: number): boolean =>
  dy >= SWIPE_MIN_PX && Math.abs(dx) <= SWIPE_MAX_DRIFT_PX;

/**
 * Swipe down on an element to minimise the call view (VOX-UI-016, mobile). Only a swipe that starts while the
 * element is scrolled to the top counts, so scrolling the tile list never closes the call. Touch events
 * are passive, so scrolling is untouched.
 */
export function useSwipeDown(
  ref: RefObject<HTMLElement | null>,
  onSwipe: (() => void) | undefined,
): void {
  useEffect(() => {
    const el = ref.current;
    if (!el || !onSwipe) return;
    let start: { x: number; y: number } | null = null;
    const down = (e: TouchEvent) => {
      const t = e.touches[0];
      start =
        t && e.touches.length === 1 && el.scrollTop <= 0 ? { x: t.clientX, y: t.clientY } : null;
    };
    const up = (e: TouchEvent) => {
      const t = e.changedTouches[0];
      const s = start;
      start = null;
      if (s && t && isSwipeDown(t.clientX - s.x, t.clientY - s.y)) onSwipe();
    };
    const cancel = () => {
      start = null;
    };
    el.addEventListener('touchstart', down, { passive: true });
    el.addEventListener('touchend', up, { passive: true });
    el.addEventListener('touchcancel', cancel, { passive: true });
    return () => {
      el.removeEventListener('touchstart', down);
      el.removeEventListener('touchend', up);
      el.removeEventListener('touchcancel', cancel);
    };
  }, [ref, onSwipe]);
}
