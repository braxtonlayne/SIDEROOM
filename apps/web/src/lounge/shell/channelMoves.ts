/**
 * Channel list reordering (LNG-CHN-008). Pure planning of the `PATCH /guilds/{id}/channels` payload.
 *
 * The sidebar shows uncategorized channels first, then the categories, so the top level is renumbered as
 * [loose channels..., categories...] and each category's children are numbered from 0.
 */
import type { Channel } from '../data';

export interface ChannelMove {
  id: string;
  position: number;
  /** New category id, or null for the top level. */
  parentId: string | null;
}

const byPos = (a: Channel, b: Channel) => a.position - b.position || a.id.localeCompare(b.id);

interface Model {
  loose: string[];
  cats: string[];
  kids: Record<string, string[]>;
}

function model(channels: readonly Channel[]): Model {
  const sorted = [...channels].sort(byPos);
  const catIds = new Set(sorted.filter((c) => c.type === 'category').map((c) => c.id));
  const m: Model = { loose: [], cats: [], kids: {} };
  for (const c of sorted) {
    if (c.type === 'category') {
      m.cats.push(c.id);
      m.kids[c.id] = [];
    }
  }
  for (const c of sorted) {
    if (c.type === 'category') continue;
    if (c.parentId && catIds.has(c.parentId)) m.kids[c.parentId]!.push(c.id);
    else m.loose.push(c.id);
  }
  return m;
}

/** Positions and parents implied by a model, to diff against the current channels. */
function flatten(m: Model): Map<string, { position: number; parentId: string | null }> {
  const out = new Map<string, { position: number; parentId: string | null }>();
  [...m.loose, ...m.cats].forEach((id, position) => out.set(id, { position, parentId: null }));
  for (const [cat, ids] of Object.entries(m.kids))
    ids.forEach((id, position) => out.set(id, { position, parentId: cat }));
  return out;
}

function diff(channels: readonly Channel[], m: Model): ChannelMove[] {
  const next = flatten(m);
  const moves: ChannelMove[] = [];
  for (const c of channels) {
    const n = next.get(c.id);
    if (!n) continue;
    const parent = c.parentId && m.kids[c.parentId] ? c.parentId : null;
    if (n.position !== c.position || n.parentId !== parent)
      moves.push({ id: c.id, position: n.position, parentId: n.parentId });
  }
  return moves;
}

function removeId(m: Model, id: string) {
  m.loose = m.loose.filter((x) => x !== id);
  m.cats = m.cats.filter((x) => x !== id);
  for (const k of Object.keys(m.kids)) m.kids[k] = m.kids[k]!.filter((x) => x !== id);
}

function insertAt(list: string[], id: string, ref: string | null, where: 'before' | 'after') {
  const i = ref ? list.indexOf(ref) : -1;
  if (i < 0) list.push(id);
  else list.splice(where === 'before' ? i : i + 1, 0, id);
}

/**
 * Dropping `dragId` on `targetId`. A channel dropped on a channel takes that channel's category and goes
 * before/after it; dropped on a category it becomes that category's first channel (or, on the category's
 * upper edge, the last uncategorized channel). A category dropped on a category goes before/after it.
 * Returns null when the drop does nothing.
 */
export function planDrop(
  channels: readonly Channel[],
  dragId: string,
  targetId: string,
  zone: 'before' | 'after' | 'into',
): ChannelMove[] | null {
  const drag = channels.find((c) => c.id === dragId);
  const target = channels.find((c) => c.id === targetId);
  if (!drag || !target || dragId === targetId) return null;
  const m = model(channels);
  const dragIsCat = drag.type === 'category';
  const targetIsCat = target.type === 'category';
  removeId(m, dragId);
  if (dragIsCat) {
    if (!targetIsCat) return null;
    m.kids[dragId] = model(channels).kids[dragId] ?? [];
    insertAt(m.cats, dragId, targetId, zone === 'before' ? 'before' : 'after');
  } else if (targetIsCat) {
    if (zone === 'before') m.loose.push(dragId);
    else m.kids[targetId]!.unshift(dragId);
  } else {
    const parent = Object.keys(m.kids).find((k) => m.kids[k]!.includes(targetId));
    const list = parent ? m.kids[parent]! : m.loose;
    insertAt(list, dragId, targetId, zone === 'before' ? 'before' : 'after');
  }
  const moves = diff(channels, m);
  return moves.length ? moves : null;
}

/** Move one channel/category a place up or down among its siblings (keyboard alternative). */
export function planStep(
  channels: readonly Channel[],
  id: string,
  delta: -1 | 1,
): ChannelMove[] | null {
  const c = channels.find((x) => x.id === id);
  if (!c) return null;
  const m = model(channels);
  const list =
    c.type === 'category' ? m.cats : (Object.values(m.kids).find((k) => k.includes(id)) ?? m.loose);
  const i = list.indexOf(id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= list.length) return null;
  [list[i], list[j]] = [list[j]!, list[i]!];
  const moves = diff(channels, m);
  return moves.length ? moves : null;
}

/** Can this channel move up/down among its siblings? */
export function canStep(channels: readonly Channel[], id: string, delta: -1 | 1): boolean {
  return planStep(channels, id, delta) !== null;
}
