// Geometry for the whiteboard: shrinking a stroke before it is shared, and finding what the eraser touches.

import { LIMITS, type DrawObject, type Pt } from './log';

/** Perpendicular distance from p to the segment a-b. */
export function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  let t = len2 === 0 ? 0 : ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** Ramer-Douglas-Peucker on a list of points. */
export function simplify(points: Pt[], epsilon: number): Pt[] {
  if (points.length < 3) return points.slice();
  const keep = new Uint8Array(points.length);
  keep[0] = 1;
  keep[points.length - 1] = 1;
  const stack: Array<[number, number]> = [[0, points.length - 1]];
  while (stack.length > 0) {
    const [lo, hi] = stack.pop() as [number, number];
    let worst = 0;
    let at = -1;
    const a = points[lo] as Pt;
    const b = points[hi] as Pt;
    for (let i = lo + 1; i < hi; i++) {
      const d = distToSegment(points[i] as Pt, a, b);
      if (d > worst) {
        worst = d;
        at = i;
      }
    }
    if (at >= 0 && worst > epsilon) {
      keep[at] = 1;
      stack.push([lo, at], [at, hi]);
    }
  }
  return points.filter((_, i) => keep[i] === 1);
}

/**
 * Turn raw pointer samples into the flat integer list a stroke entry carries: drop near-duplicates, simplify,
 * and tighten until it fits the point limit. A single tap becomes a two-point dot.
 */
export function packStroke(raw: Pt[]): number[] {
  if (raw.length === 0) return [];
  const pts: Pt[] = [];
  for (const p of raw) {
    const q: Pt = [Math.round(p[0]), Math.round(p[1])];
    const last = pts[pts.length - 1];
    if (!last || Math.hypot(q[0] - last[0], q[1] - last[1]) >= 1) pts.push(q);
  }
  if (pts.length === 1) pts.push([(pts[0] as Pt)[0], (pts[0] as Pt)[1]]);
  let eps = 0.8;
  let out = simplify(pts, eps);
  while (out.length > LIMITS.maxPoints) {
    eps *= 1.6;
    out = simplify(pts, eps);
  }
  return out.flatMap((p) => [p[0], p[1]]);
}

export interface Bounds {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Text width is approximate (no canvas here); the renderer measures for real. */
export function textBounds(o: Extract<DrawObject, { k: 'text' }>): Bounds {
  const lines = o.text.split('\n');
  const longest = Math.max(...lines.map((l) => l.length));
  return {
    x0: o.at[0],
    y0: o.at[1],
    x1: o.at[0] + longest * o.size * 0.55,
    y1: o.at[1] + lines.length * o.size * 1.25,
  };
}

export function bounds(o: DrawObject): Bounds {
  switch (o.k) {
    case 'stroke': {
      let x0 = Infinity;
      let y0 = Infinity;
      let x1 = -Infinity;
      let y1 = -Infinity;
      for (let i = 0; i < o.pts.length; i += 2) {
        const x = o.pts[i] as number;
        const y = o.pts[i + 1] as number;
        x0 = Math.min(x0, x);
        x1 = Math.max(x1, x);
        y0 = Math.min(y0, y);
        y1 = Math.max(y1, y);
      }
      return { x0, y0, x1, y1 };
    }
    case 'shape':
      return {
        x0: Math.min(o.a[0], o.b[0]),
        y0: Math.min(o.a[1], o.b[1]),
        x1: Math.max(o.a[0], o.b[0]),
        y1: Math.max(o.a[1], o.b[1]),
      };
    case 'text':
      return textBounds(o);
    case 'image':
      return { x0: o.at[0], y0: o.at[1], x1: o.at[0] + o.size[0], y1: o.at[1] + o.size[1] };
  }
}

const inside = (p: Pt, b: Bounds, pad: number): boolean =>
  p[0] >= b.x0 - pad && p[0] <= b.x1 + pad && p[1] >= b.y0 - pad && p[1] <= b.y1 + pad;

/** Whether an eraser dot of `radius` at p touches the object. */
export function hits(o: DrawObject, p: Pt, radius: number): boolean {
  const pad = radius + (o.k === 'stroke' || o.k === 'shape' ? o.w / 2 : 0);
  if (!inside(p, bounds(o), pad)) return false;
  switch (o.k) {
    case 'stroke': {
      if (o.pts.length === 2)
        return Math.hypot(p[0] - (o.pts[0] as number), p[1] - (o.pts[1] as number)) <= pad;
      for (let i = 0; i + 3 < o.pts.length; i += 2) {
        const a: Pt = [o.pts[i] as number, o.pts[i + 1] as number];
        const b: Pt = [o.pts[i + 2] as number, o.pts[i + 3] as number];
        if (distToSegment(p, a, b) <= pad) return true;
      }
      return false;
    }
    case 'shape': {
      if (o.s === 'line' || o.s === 'arrow') return distToSegment(p, o.a, o.b) <= pad;
      if (o.fill) return true;
      const b = bounds(o);
      if (o.s === 'rect') {
        const near = (d: number) => d <= pad;
        const onX = p[0] >= b.x0 - pad && p[0] <= b.x1 + pad;
        const onY = p[1] >= b.y0 - pad && p[1] <= b.y1 + pad;
        return (
          (onY && (near(Math.abs(p[0] - b.x0)) || near(Math.abs(p[0] - b.x1)))) ||
          (onX && (near(Math.abs(p[1] - b.y0)) || near(Math.abs(p[1] - b.y1))))
        );
      }
      // Ellipse outline: compare the normalised radius with 1.
      const rx = Math.max(1, (b.x1 - b.x0) / 2);
      const ry = Math.max(1, (b.y1 - b.y0) / 2);
      const nx = (p[0] - (b.x0 + b.x1) / 2) / rx;
      const ny = (p[1] - (b.y0 + b.y1) / 2) / ry;
      const r = Math.hypot(nx, ny);
      return Math.abs(r - 1) * Math.min(rx, ry) <= pad;
    }
    case 'text':
    case 'image':
      return true;
  }
}

/** The topmost object under the eraser, or null. `objects` is in paint order. */
export function pick(
  objects: DrawObject[],
  pageId: string,
  p: Pt,
  radius: number,
): DrawObject | null {
  for (let i = objects.length - 1; i >= 0; i--) {
    const o = objects[i] as DrawObject;
    if (o.pg === pageId && hits(o, p, radius)) return o;
  }
  return null;
}
