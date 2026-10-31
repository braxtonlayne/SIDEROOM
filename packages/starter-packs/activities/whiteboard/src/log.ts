// The whiteboard's append-only log (ACT-WB-001). Every drawing action is an immutable entry written once under
// its own key, so concurrent writers never collide (last-writer-wins per key is then harmless). The visible
// board is a pure fold of the entries in a total order. The host compacts the log into a snapshot (see sync.ts);
// `compactEntries` here is the pure part of that.

export const PAGE_W = 1600;
export const PAGE_H = 900;
/** The page every board starts with; it can be deleted like any other when another page exists. */
export const HOME_PAGE = 'home';

export type Pt = readonly [number, number];
export type ShapeKind = 'rect' | 'ellipse' | 'line' | 'arrow';

export type Payload =
  | { k: 'stroke'; pg: string; c: string; w: number; hl: boolean; pts: number[] }
  | {
      k: 'shape';
      pg: string;
      s: ShapeKind;
      c: string;
      w: number;
      fill: boolean;
      a: Pt;
      b: Pt;
    }
  | { k: 'text'; pg: string; c: string; size: number; at: Pt; text: string }
  | { k: 'image'; pg: string; src: string; at: Pt; size: Pt }
  | { k: 'clear'; pg: string }
  | { k: 'erase'; t: string }
  | { k: 'undo'; t: string }
  | { k: 'redo'; t: string }
  | { k: 'page'; name: string }
  | { k: 'rename'; t: string; name: string }
  | { k: 'pagedel'; t: string };

/** One log entry. `a` is the author's member id, `n` their counter (from 1), `ts` a Lamport clock. */
export interface Entry {
  a: string;
  n: number;
  ts: number;
  p: Payload;
}

export const entryId = (e: { a: string; n: number }): string => `${e.a}:${e.n}`;

/** The total order: Lamport time, then author, then counter. */
export function compareEntries(x: Entry, y: Entry): number {
  if (x.ts !== y.ts) return x.ts - y.ts;
  if (x.a !== y.a) return x.a < y.a ? -1 : 1;
  return x.n - y.n;
}

export type DrawPayload = Extract<Payload, { k: 'stroke' | 'shape' | 'text' | 'image' }>;
export type DrawObject = DrawPayload & { id: string; author: string };

export interface PageInfo {
  id: string;
  name: string;
}

export interface View {
  pages: PageInfo[];
  /** Visible objects in paint order, every page. */
  objects: DrawObject[];
}

const isDrawing = (p: Payload): p is DrawPayload =>
  p.k === 'stroke' || p.k === 'shape' || p.k === 'text' || p.k === 'image';

// --- validation: everything from the wire is untrusted ---------------------------------------------------

export const LIMITS = {
  maxPoints: 2000,
  maxText: 500,
  maxName: 40,
  maxImageChars: 160_000,
  coordMin: -200,
  coordMax: 2200,
};

const COLOR = /^#[0-9a-fA-F]{6}$/;
const ID = /^[A-Za-z0-9_.:-]{1,64}$/;
const IMAGE_SRC =
  /^(https:\/\/[^\s"'<>]{1,2000}|data:image\/(png|jpeg|webp|gif);base64,[A-Za-z0-9+/=]+)$/;

const num = (v: unknown, lo: number, hi: number): number | null =>
  typeof v === 'number' && Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : null;
const coord = (v: unknown): number | null => num(v, LIMITS.coordMin, LIMITS.coordMax);
function pt(v: unknown): Pt | null {
  if (!Array.isArray(v) || v.length !== 2) return null;
  const x = coord(v[0]);
  const y = coord(v[1]);
  return x === null || y === null ? null : [x, y];
}
const str = (v: unknown, max: number): string | null =>
  typeof v === 'string' && v.length <= max ? v : null;
const color = (v: unknown): string | null => (typeof v === 'string' && COLOR.test(v) ? v : null);
const idOf = (v: unknown): string | null => (typeof v === 'string' && ID.test(v) ? v : null);

function parsePayload(raw: unknown): Payload | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const r = raw as Record<string, unknown>;
  switch (r.k) {
    case 'stroke': {
      const pg = idOf(r.pg);
      const c = color(r.c);
      const w = num(r.w, 1, 64);
      if (!pg || !c || w === null || !Array.isArray(r.pts)) return null;
      if (r.pts.length < 2 || r.pts.length % 2 !== 0 || r.pts.length > LIMITS.maxPoints * 2) {
        return null;
      }
      const pts: number[] = [];
      for (const v of r.pts) {
        const q = coord(v);
        if (q === null) return null;
        pts.push(q);
      }
      return { k: 'stroke', pg, c, w, hl: r.hl === true, pts };
    }
    case 'shape': {
      const pg = idOf(r.pg);
      const c = color(r.c);
      const w = num(r.w, 1, 64);
      const a = pt(r.a);
      const b = pt(r.b);
      const s = r.s;
      if (!pg || !c || w === null || !a || !b) return null;
      if (s !== 'rect' && s !== 'ellipse' && s !== 'line' && s !== 'arrow') return null;
      return { k: 'shape', pg, s, c, w, fill: r.fill === true, a, b };
    }
    case 'text': {
      const pg = idOf(r.pg);
      const c = color(r.c);
      const size = num(r.size, 8, 200);
      const at = pt(r.at);
      const text = str(r.text, LIMITS.maxText);
      if (!pg || !c || size === null || !at || text === null || text.trim() === '') return null;
      return { k: 'text', pg, c, size, at, text };
    }
    case 'image': {
      const pg = idOf(r.pg);
      const at = pt(r.at);
      const size = pt(r.size);
      const src = str(r.src, LIMITS.maxImageChars);
      if (!pg || !at || !size || !src || !IMAGE_SRC.test(src)) return null;
      if (size[0] < 1 || size[1] < 1) return null;
      return { k: 'image', pg, src, at, size };
    }
    case 'clear': {
      const pg = idOf(r.pg);
      return pg ? { k: 'clear', pg } : null;
    }
    case 'erase':
    case 'undo':
    case 'redo':
    case 'pagedel': {
      const t = idOf(r.t);
      return t ? ({ k: r.k, t } as Payload) : null;
    }
    case 'page': {
      const name = str(r.name, LIMITS.maxName);
      return name === null ? null : { k: 'page', name };
    }
    case 'rename': {
      const t = idOf(r.t);
      const name = str(r.name, LIMITS.maxName);
      return t && name !== null ? { k: 'rename', t, name } : null;
    }
    default:
      return null;
  }
}

/** Parse an untrusted value into an entry, or `null`. Numbers are rounded and clamped. */
export function parseEntry(raw: unknown): Entry | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const a = idOf(r.a);
  const n = typeof r.n === 'number' && Number.isInteger(r.n) && r.n >= 1 ? r.n : null;
  const ts = typeof r.ts === 'number' && Number.isInteger(r.ts) && r.ts >= 0 ? r.ts : null;
  if (!a || n === null || ts === null) return null;
  const p = parsePayload(r.p);
  return p ? { a, n, ts, p } : null;
}

// --- the fold ----------------------------------------------------------------------------------------------

export interface Folded {
  view: View;
  /** The entries that still matter, in order; what compaction keeps. */
  survivors: Entry[];
  /** Ids currently undone (redo can bring them back). */
  undone: Set<string>;
  /** Ids of the clears in effect. */
  clears: Set<string>;
}

/**
 * Fold entries (any order; duplicates by id collapse) into the visible board.
 *
 * - An author can undo or redo only their own entries; an undo marker cannot itself be undone.
 * - `erase` removes any drawn object and cannot be redone.
 * - A `clear` removes the page's objects that come before it in the order, unless the clear is undone.
 * - Deleting a page hides it and its objects; the last remaining page cannot be deleted.
 * - A `rename` is last-writer-wins by the order.
 */
export function fold(input: Iterable<Entry>): Folded {
  const byId = new Map<string, Entry>();
  for (const e of input) byId.set(entryId(e), e);
  const list = [...byId.values()].sort(compareEntries);

  const undone = new Set<string>();
  const erased = new Set<string>();
  for (const e of list) {
    const p = e.p;
    if (p.k === 'undo' || p.k === 'redo') {
      const target = byId.get(p.t);
      if (!target || target.a !== e.a) continue;
      const tk = target.p.k;
      if (tk === 'undo' || tk === 'redo' || tk === 'erase') continue;
      if (p.k === 'undo') undone.add(p.t);
      else undone.delete(p.t);
    } else if (p.k === 'erase') {
      const target = byId.get(p.t);
      if (target && isDrawing(target.p)) erased.add(p.t);
    }
  }
  const active = (e: Entry): boolean => !undone.has(entryId(e)) && !erased.has(entryId(e));

  // Pages in order of creation; the home page exists from the start.
  const names = new Map<string, string>([[HOME_PAGE, 'Page 1']]);
  const order: string[] = [HOME_PAGE];
  const lastRename = new Map<string, Entry>();
  const addEntries = new Map<string, Entry>();
  let homeDeletion: Entry | null = null;
  for (const e of list) {
    const p = e.p;
    if (p.k === 'page' && active(e)) {
      names.set(entryId(e), p.name);
      order.push(entryId(e));
      addEntries.set(entryId(e), e);
    } else if (p.k === 'rename' && names.has(p.t)) {
      names.set(p.t, p.name);
      lastRename.set(p.t, e);
    } else if (p.k === 'pagedel' && names.has(p.t) && names.size > 1) {
      names.delete(p.t);
      if (p.t === HOME_PAGE) homeDeletion = e;
    }
  }

  const clearedBy = new Map<string, Entry>();
  const clears = new Set<string>();
  for (const e of list) {
    if (e.p.k === 'clear' && active(e)) {
      clearedBy.set(e.p.pg, e);
      clears.add(entryId(e));
    }
  }

  const objects: DrawObject[] = [];
  const kept: Entry[] = [];
  for (const e of list) {
    const p = e.p;
    if (!isDrawing(p) || !active(e) || !names.has(p.pg)) continue;
    const clear = clearedBy.get(p.pg);
    if (clear && compareEntries(e, clear) < 0) continue;
    objects.push({ ...p, id: entryId(e), author: e.a });
    kept.push(e);
  }

  const pages: PageInfo[] = order
    .filter((id) => names.has(id))
    .map((id) => ({ id, name: names.get(id) ?? '' }));
  for (const [id, e] of addEntries) if (names.has(id)) kept.push(e);
  for (const [id, e] of lastRename) if (names.has(id)) kept.push(e);
  if (homeDeletion) kept.push(homeDeletion);
  kept.sort(compareEntries);
  return { view: { pages, objects }, survivors: kept, undone, clears };
}

/** The pure core of compaction: the entries to keep so that `fold(kept)` shows the same board. */
export function compactEntries(input: Iterable<Entry>): Entry[] {
  return fold(input).survivors;
}

/** Whether two views show the same board (pages, ids and paint order). */
export function sameView(a: View, b: View): boolean {
  if (a.pages.length !== b.pages.length || a.objects.length !== b.objects.length) return false;
  for (let i = 0; i < a.pages.length; i++) {
    if (a.pages[i]?.id !== b.pages[i]?.id || a.pages[i]?.name !== b.pages[i]?.name) return false;
  }
  for (let i = 0; i < a.objects.length; i++) {
    if (a.objects[i]?.id !== b.objects[i]?.id) return false;
  }
  return true;
}

/** The author's next undo: their latest drawn object or clear that is in effect. */
export function undoTarget(entries: Iterable<Entry>, me: string): string | null {
  const all = [...entries];
  const { survivors, clears } = fold(all);
  let best: Entry | null = null;
  const consider = (e: Entry) => {
    if (e.a === me && (!best || compareEntries(e, best) > 0)) best = e;
  };
  for (const e of survivors) if (isDrawing(e.p)) consider(e);
  for (const e of all) if (clears.has(entryId(e))) consider(e);
  return best ? entryId(best) : null;
}

/** The author's most recently undone entry that can be brought back, or null. */
export function redoTarget(entries: Iterable<Entry>, me: string): string | null {
  const all = [...entries];
  const { undone } = fold(all);
  let best: Entry | null = null;
  for (const e of all) {
    if (e.a !== me || !undone.has(entryId(e))) continue;
    if (!best || compareEntries(e, best) > 0) best = e;
  }
  return best ? entryId(best) : null;
}
