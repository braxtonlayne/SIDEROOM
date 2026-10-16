import { useEffect, type RefObject } from 'react';
import { setTileViewport } from '../../voice/video';

/**
 * Report how big a remote video tile is drawn, so the SFU sends the smallest simulcast layer that covers it
 * (VOX-CLI-006: a small tile gets the thumbnail layer, the spotlight gets the full one). Sizes are CSS pixels
 * times the device pixel ratio (at most 2), rounded up to 40 so a resize drag does not flood the controller.
 */
export function useTileViewport(
  ref: RefObject<HTMLElement | null>,
  userId: string,
  source: 'camera' | 'screen',
  active: boolean,
): void {
  useEffect(() => {
    const el = ref.current;
    if (!active || !el) return;
    const report = (w: number, h: number) => {
      const dpr = Math.min(globalThis.devicePixelRatio || 1, 2);
      const snap = (n: number) => Math.max(40, Math.ceil((n * dpr) / 40) * 40);
      if (w > 0 && h > 0) setTileViewport(userId, source, { width: snap(w), height: snap(h) });
    };
    report(el.clientWidth, el.clientHeight);
    if (typeof ResizeObserver === 'undefined') return () => setTileViewport(userId, source, null);
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (r) report(r.width, r.height);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      setTileViewport(userId, source, null);
    };
  }, [ref, userId, source, active]);
}
