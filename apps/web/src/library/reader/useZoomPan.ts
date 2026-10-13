// Zoom and pan for the paged reader (LIB-RDR-005): wheel and double-click zoom, drag to pan while zoomed,
// two-finger pinch. The state is a CSS transform on the stage; it resets whenever `resetKey` changes.
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import { clamp } from './model';

export const MIN_SCALE = 1;
export const MAX_SCALE = 4;

export interface ZoomState {
  scale: number;
  x: number;
  y: number;
}
const IDENTITY: ZoomState = { scale: 1, x: 0, y: 0 };

export function useZoomPan(
  target: RefObject<HTMLElement | null>,
  enabled: boolean,
  resetKey: unknown,
) {
  const [z, setZ] = useState<ZoomState>(IDENTITY);
  const zRef = useRef(z);
  zRef.current = z;
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const pinch = useRef<{ dist: number; scale: number } | null>(null);
  const moved = useRef(false);

  useEffect(() => setZ(IDENTITY), [resetKey, enabled]);

  const zoomTo = useCallback((scale: number, ox = 0, oy = 0) => {
    setZ((p) => {
      const s = clamp(scale, MIN_SCALE, MAX_SCALE);
      if (s === MIN_SCALE) return IDENTITY;
      // Keep the point under the cursor fixed (offsets are from the stage centre).
      const k = s / p.scale;
      return { scale: s, x: ox - (ox - p.x) * k, y: oy - (oy - p.y) * k };
    });
  }, []);

  // Wheel needs a non-passive native listener so the page does not scroll while zooming.
  useEffect(() => {
    const el = target.current;
    if (!el || !enabled) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      zoomTo(
        zRef.current.scale * Math.exp(-e.deltaY * 0.002),
        e.clientX - (r.left + r.width / 2),
        e.clientY - (r.top + r.height / 2),
      );
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [target, enabled, zoomTo]);

  const onPointerDown = (e: React.PointerEvent) => {
    if (!enabled) return;
    moved.current = false;
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()] as [
        { x: number; y: number },
        { x: number; y: number },
      ];
      pinch.current = { dist: Math.hypot(a.x - b.x, a.y - b.y), scale: zRef.current.scale };
    }
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const prev = pointers.current.get(e.pointerId);
    if (!enabled || !prev) return;
    const cur = { x: e.clientX, y: e.clientY };
    pointers.current.set(e.pointerId, cur);
    if (Math.abs(cur.x - prev.x) + Math.abs(cur.y - prev.y) > 0) moved.current = true;
    if (pointers.current.size === 2 && pinch.current) {
      const [a, b] = [...pointers.current.values()] as [
        { x: number; y: number },
        { x: number; y: number },
      ];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      zoomTo(pinch.current.scale * (d / (pinch.current.dist || 1)));
    } else if (zRef.current.scale > 1) {
      setZ((p) => ({ ...p, x: p.x + (cur.x - prev.x), y: p.y + (cur.y - prev.y) }));
    }
  };
  const end = (e: React.PointerEvent) => {
    pointers.current.delete(e.pointerId);
    if (pointers.current.size < 2) pinch.current = null;
  };

  const onDoubleClick = (e: React.MouseEvent) => {
    if (!enabled) return;
    const el = target.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (zRef.current.scale > 1) zoomTo(1);
    else zoomTo(2.5, e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2));
  };

  return {
    zoom: z,
    zoomed: z.scale > 1,
    /** True when the last pointer gesture moved: callers use it to ignore the click that follows a drag. */
    wasDrag: () => moved.current,
    zoomBy: (f: number) => zoomTo(zRef.current.scale * f),
    reset: () => setZ(IDENTITY),
    bind: { onPointerDown, onPointerMove, onPointerUp: end, onPointerCancel: end, onDoubleClick },
  };
}
