/**
 * UTL-SHL-001: the pure part of the gadget layer. A "layout" is what the account remembers about the tools:
 * which are popped out (a rectangle each, in CSS pixels from the inline-start / block-start corner of the
 * viewport, so it mirrors in right-to-left) and which are pinned to the side rail. Everything here is a pure
 * function over plain data, so the limits and the clamping are unit-tested without a DOM.
 */
export const MAX_GADGETS = 6;
export const MAX_PINS = 12;

export interface GadgetRect {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Gadget extends GadgetRect {
  tool: string;
  /** Stacking order; the highest is in front. Renumbered 1..n whenever it changes. */
  z: number;
}
export interface Layout {
  gadgets: Gadget[];
  pins: string[];
}
export interface Viewport {
  w: number;
  h: number;
}
export interface Mins {
  minWidth: number;
  minHeight: number;
}

export const EMPTY_LAYOUT: Layout = { gadgets: [], pins: [] };
export const isEmptyLayout = (l: Layout): boolean => l.gadgets.length === 0 && l.pins.length === 0;

const num = (v: unknown, d: number): number => (typeof v === 'number' && Number.isFinite(v) ? v : d);
const TOOL_ID = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

/** Reads whatever storage or the account blob holds; never throws, never returns more than the limits allow. */
export function sanitizeLayout(raw: unknown): Layout {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return EMPTY_LAYOUT;
  const r = raw as Record<string, unknown>;
  const seen = new Set<string>();
  const gadgets: Gadget[] = [];
  if (Array.isArray(r.gadgets))
    for (const g of r.gadgets) {
      if (typeof g !== 'object' || g === null) continue;
      const o = g as Record<string, unknown>;
      if (typeof o.tool !== 'string' || !TOOL_ID.test(o.tool) || seen.has(o.tool)) continue;
      if (gadgets.length >= MAX_GADGETS) break;
      seen.add(o.tool);
      gadgets.push({
        tool: o.tool,
        x: Math.round(num(o.x, 24)),
        y: Math.round(num(o.y, 24)),
        w: Math.max(1, Math.round(num(o.w, 360))),
        h: Math.max(1, Math.round(num(o.h, 420))),
        z: Math.round(num(o.z, gadgets.length + 1)),
      });
    }
  const pinSeen = new Set<string>();
  const pins: string[] = [];
  if (Array.isArray(r.pins))
    for (const p of r.pins) {
      if (typeof p !== 'string' || !TOOL_ID.test(p) || pinSeen.has(p) || pins.length >= MAX_PINS)
        continue;
      pinSeen.add(p);
      pins.push(p);
    }
  return { gadgets: renumber(gadgets), pins };
}

function renumber(list: Gadget[]): Gadget[] {
  return [...list]
    .sort((a, b) => a.z - b.z)
    .map((g, i) => ({ ...g, z: i + 1 }));
}

/** Keeps a rectangle usable: at least the minimum size, no bigger than the viewport, title bar reachable. */
export function clampRect(rect: GadgetRect, vp: Viewport, mins: Mins): GadgetRect {
  const w = Math.min(Math.max(rect.w, mins.minWidth), Math.max(mins.minWidth, vp.w));
  const h = Math.min(Math.max(rect.h, mins.minHeight), Math.max(mins.minHeight, vp.h));
  const x = Math.min(Math.max(0, rect.x), Math.max(0, vp.w - w));
  const y = Math.min(Math.max(0, rect.y), Math.max(0, vp.h - h));
  return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
}

export type OpenResult =
  | { ok: true; layout: Layout }
  | { ok: false; reason: 'limit'; layout: Layout };

/** Pops a tool out. Already open: it is raised, not duplicated. At the limit nothing changes. */
export function openGadget(
  layout: Layout,
  tool: string,
  size: { width: number; height: number } & Mins,
  vp: Viewport,
): OpenResult {
  const existing = layout.gadgets.find((g) => g.tool === tool);
  if (existing) return { ok: true, layout: raiseGadget(layout, tool) };
  if (layout.gadgets.length >= MAX_GADGETS) return { ok: false, reason: 'limit', layout };
  const n = layout.gadgets.length;
  const w = Math.min(size.width, Math.max(size.minWidth, vp.w - 32));
  const h = Math.min(size.height, Math.max(size.minHeight, vp.h - 32));
  // Cascade from the inline-start corner so a second gadget never hides the first one's title bar.
  const rect = clampRect({ x: 72 + n * 28, y: 56 + n * 28, w, h }, vp, size);
  const top = layout.gadgets.reduce((m, g) => Math.max(m, g.z), 0);
  return {
    ok: true,
    layout: { ...layout, gadgets: [...layout.gadgets, { tool, ...rect, z: top + 1 }] },
  };
}

export function closeGadget(layout: Layout, tool: string): Layout {
  if (!layout.gadgets.some((g) => g.tool === tool)) return layout;
  return { ...layout, gadgets: renumber(layout.gadgets.filter((g) => g.tool !== tool)) };
}

export function raiseGadget(layout: Layout, tool: string): Layout {
  const g = layout.gadgets.find((x) => x.tool === tool);
  if (!g) return layout;
  const top = layout.gadgets.reduce((m, x) => Math.max(m, x.z), 0);
  if (g.z === top) return layout;
  return {
    ...layout,
    gadgets: renumber(layout.gadgets.map((x) => (x.tool === tool ? { ...x, z: top + 1 } : x))),
  };
}

export function setGadgetRect(
  layout: Layout,
  tool: string,
  rect: GadgetRect,
  vp: Viewport,
  mins: Mins,
): Layout {
  const next = clampRect(rect, vp, mins);
  let changed = false;
  const gadgets = layout.gadgets.map((g) => {
    if (g.tool !== tool) return g;
    if (g.x === next.x && g.y === next.y && g.w === next.w && g.h === next.h) return g;
    changed = true;
    return { ...g, ...next };
  });
  return changed ? { ...layout, gadgets } : layout;
}

export function pinTool(layout: Layout, tool: string): Layout {
  if (layout.pins.includes(tool) || layout.pins.length >= MAX_PINS) return layout;
  return { ...layout, pins: [...layout.pins, tool] };
}

export function unpinTool(layout: Layout, tool: string): Layout {
  return layout.pins.includes(tool)
    ? { ...layout, pins: layout.pins.filter((p) => p !== tool) }
    : layout;
}
