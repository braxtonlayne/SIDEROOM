export interface Size {
  w: number;
  h: number;
}
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The attribute that marks a region as private (a DM message list, a call tile): it is covered by default. */
export const PRIVATE_ATTR = 'data-sr-private';

/** True when the frame has the viewport's proportions within 2 percent (a tab capture does; a shared window usually does not). */
export function frameMatchesViewport(viewport: Size, frame: Size): boolean {
  if (!(viewport.w > 0 && viewport.h > 0 && frame.w > 0 && frame.h > 0)) return false;
  const a = viewport.w / viewport.h;
  const b = frame.w / frame.h;
  return Math.abs(a - b) / a <= 0.02;
}

/** DOM rectangles (CSS px in the viewport) scaled to frame pixels, clipped to the frame; empty ones dropped. */
export function coversFromRects(rects: readonly Rect[], viewport: Size, frame: Size): Rect[] {
  if (!(viewport.w > 0 && viewport.h > 0 && frame.w > 0 && frame.h > 0)) return [];
  const sx = frame.w / viewport.w;
  const sy = frame.h / viewport.h;
  const out: Rect[] = [];
  for (const r of rects) {
    const x0 = Math.max(0, Math.floor(r.x * sx));
    const y0 = Math.max(0, Math.floor(r.y * sy));
    const x1 = Math.min(frame.w, Math.ceil((r.x + r.w) * sx));
    const y1 = Math.min(frame.h, Math.ceil((r.y + r.h) * sy));
    if (x1 - x0 > 0 && y1 - y0 > 0) out.push({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
  }
  return out;
}

/** The viewport rectangles of every visible `[data-sr-private]` under `root`. Call it before the capture. */
export function collectPrivateRects(root: ParentNode = document): Rect[] {
  const out: Rect[] = [];
  for (const el of Array.from(root.querySelectorAll(`[${PRIVATE_ATTR}]`))) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) out.push({ x: r.left, y: r.top, w: r.width, h: r.height });
  }
  return out;
}
