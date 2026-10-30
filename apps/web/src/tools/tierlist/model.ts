/**
 * UTL-COL-001: the tier list as keys of a tool room. Every key holds one small object, so two people editing
 * different things never collide, and a move is one key (`item.<id>`): last writer wins per key.
 *
 *   tier.<id>  { name, hue, pos }
 *   item.<id>  { text?, emoji?, img?, tier, pos }       tier '' = the unranked pool
 *
 * `img` is a reference, never a URL: `{ k: 'mount', mount, key }` is presigned by each viewer for themselves
 * (so the storage scopes apply per viewer) and `{ k: 'path', path }` is a same-origin path.
 */

export type Hue = 'danger' | 'warning' | 'success' | 'info' | 'accent' | 'muted';
export const HUES: readonly Hue[] = ['danger', 'warning', 'success', 'info', 'accent', 'muted'];

export type ImgRef = { k: 'mount'; mount: string; key: string } | { k: 'path'; path: string };

export interface Tier {
  id: string;
  name: string;
  hue: Hue;
  pos: number;
}
export interface TierItem {
  id: string;
  text: string;
  emoji: string;
  img: ImgRef | null;
  tier: string;
  pos: number;
}

export const MAX_TIERS = 12;
export const MAX_ITEMS = 120;
export const MAX_TEXT = 60;
export const MAX_TIER_NAME = 24;
export const UNRANKED = '';

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** A short random id (keys are 64 characters at most). */
export function newId(): string {
  const a = new Uint8Array(5);
  crypto.getRandomValues(a);
  return Array.from(a, (b) => b.toString(36).padStart(2, '0')).join('');
}

function parseImg(v: unknown): ImgRef | null {
  if (!isObj(v)) return null;
  if (v.k === 'mount' && typeof v.mount === 'string' && typeof v.key === 'string')
    return { k: 'mount', mount: v.mount, key: v.key };
  if (v.k === 'path' && typeof v.path === 'string') return { k: 'path', path: v.path };
  return null;
}

/** Tiers and items out of a room's state; anything malformed is skipped, never thrown on. */
export function parseTierList(state: Record<string, unknown>): {
  tiers: Tier[];
  items: TierItem[];
} {
  const tiers: Tier[] = [];
  const items: TierItem[] = [];
  for (const [key, v] of Object.entries(state)) {
    if (!isObj(v)) continue;
    const pos = typeof v.pos === 'number' && Number.isFinite(v.pos) ? v.pos : 0;
    if (key.startsWith('tier.')) {
      const hue = HUES.includes(v.hue as Hue) ? (v.hue as Hue) : 'muted';
      tiers.push({ id: key.slice(5), name: typeof v.name === 'string' ? v.name : '', hue, pos });
    } else if (key.startsWith('item.')) {
      items.push({
        id: key.slice(5),
        text: typeof v.text === 'string' ? v.text : '',
        emoji: typeof v.emoji === 'string' ? v.emoji : '',
        img: parseImg(v.img),
        tier: typeof v.tier === 'string' ? v.tier : UNRANKED,
        pos,
      });
    }
  }
  const byPos = (a: { pos: number; id: string }, b: { pos: number; id: string }) =>
    a.pos - b.pos || (a.id < b.id ? -1 : 1);
  return { tiers: tiers.sort(byPos), items: items.sort(byPos) };
}

/** The classic starting rows. */
export function defaultTiers(names: readonly string[]): Record<string, unknown> {
  const hues: Hue[] = ['danger', 'warning', 'success', 'info', 'accent', 'muted'];
  const out: Record<string, unknown> = {};
  names.forEach((name, i) => {
    out[`tier.${newId()}`] = { name, hue: hues[i % hues.length], pos: (i + 1) * 1024 };
  });
  return out;
}

/** Items of one tier (or the pool) in order. */
export const itemsIn = (items: readonly TierItem[], tier: string): TierItem[] =>
  items.filter((i) => i.tier === tier);

/**
 * The position that puts an item at `index` among `siblings` (already without the moved item): the middle
 * of its neighbours, so one move changes one key.
 */
export function positionAt(siblings: readonly { pos: number }[], index: number): number {
  const i = Math.max(0, Math.min(index, siblings.length));
  const before = i > 0 ? siblings[i - 1]!.pos : undefined;
  const after = i < siblings.length ? siblings[i]!.pos : undefined;
  if (before === undefined && after === undefined) return 1024;
  if (before === undefined) return after! - 1024;
  if (after === undefined) return before + 1024;
  return (before + after) / 2;
}

/** The patch that moves an item into `tier` at `index`. `null` when the item does not exist. */
export function moveItem(
  state: Record<string, unknown>,
  itemId: string,
  tier: string,
  index: number,
): Record<string, unknown> | null {
  const { items } = parseTierList(state);
  const item = items.find((i) => i.id === itemId);
  if (!item) return null;
  const siblings = itemsIn(items, tier).filter((i) => i.id !== itemId);
  return { [`item.${itemId}`]: toValue({ ...item, tier, pos: positionAt(siblings, index) }) };
}

export function toValue(i: Omit<TierItem, 'id'>): Record<string, unknown> {
  return {
    ...(i.text ? { text: i.text } : {}),
    ...(i.emoji ? { emoji: i.emoji } : {}),
    ...(i.img ? { img: i.img } : {}),
    tier: i.tier,
    pos: i.pos,
  };
}

/** The patch that adds an item at the end of the pool; `null` when the list is full or the item is empty. */
export function addItem(
  state: Record<string, unknown>,
  input: { text?: string; emoji?: string; img?: ImgRef | null },
): Record<string, unknown> | null {
  const text = (input.text ?? '').trim().slice(0, MAX_TEXT);
  const emoji = (input.emoji ?? '').trim().slice(0, 8);
  const img = input.img ?? null;
  if (!text && !emoji && !img) return null;
  const { items } = parseTierList(state);
  if (items.length >= MAX_ITEMS) return null;
  const pool = itemsIn(items, UNRANKED);
  return {
    [`item.${newId()}`]: toValue({
      text,
      emoji,
      img,
      tier: UNRANKED,
      pos: positionAt(pool, pool.length),
    }),
  };
}

/** Add a row at the bottom; `null` when full. */
export function addTier(
  state: Record<string, unknown>,
  name: string,
): Record<string, unknown> | null {
  const { tiers } = parseTierList(state);
  if (tiers.length >= MAX_TIERS) return null;
  const n = name.trim().slice(0, MAX_TIER_NAME);
  if (!n) return null;
  const last = tiers[tiers.length - 1];
  return {
    [`tier.${newId()}`]: {
      name: n,
      hue: HUES[tiers.length % HUES.length],
      pos: last ? last.pos + 1024 : 1024,
    },
  };
}

/** Remove a row; its items go back to the pool. */
export function removeTier(
  state: Record<string, unknown>,
  tierId: string,
): Record<string, unknown> {
  const { items } = parseTierList(state);
  const set: Record<string, unknown> = { [`tier.${tierId}`]: null };
  const pool = itemsIn(items, UNRANKED);
  let at = pool.length;
  for (const it of itemsIn(items, tierId)) {
    set[`item.${it.id}`] = toValue({ ...it, tier: UNRANKED, pos: positionAt(pool, at++) });
  }
  return set;
}

/** The result as plain text, one line per row ("S: pizza, tacos"), for posting as a message. */
export function tierListText(
  title: string,
  state: Record<string, unknown>,
  unrankedLabel: string,
): string {
  const { tiers, items } = parseTierList(state);
  const label = (i: TierItem) => [i.emoji, i.text].filter(Boolean).join(' ') || '[image]';
  const lines = tiers.map((t) => `**${t.name}**: ${itemsIn(items, t.id).map(label).join(', ')}`);
  const pool = itemsIn(items, UNRANKED);
  if (pool.length) lines.push(`*${unrankedLabel}*: ${pool.map(label).join(', ')}`);
  return [title ? `**${title}**` : '', ...lines].filter(Boolean).join('\n');
}
