/**
 * Search box helpers (LNG-SRC-004/005/006): filter suggestions for the token being typed, grouping of hits
 * by channel, sort options and the recent-query history. Pure or tiny-state, so they are unit-tested.
 */

export type SearchSort = 'new' | 'old' | 'relevant';

export const SORT_LABELS: Record<SearchSort, string> = {
  new: 'Newest',
  old: 'Oldest',
  relevant: 'Most relevant',
};

/** Query params the search endpoints take for a sort choice. */
export function sortParams(sort: SearchSort): Record<string, string> {
  if (sort === 'relevant') return { sort_by: 'relevance', sort_order: 'desc' };
  return { sort_by: 'timestamp', sort_order: sort === 'old' ? 'asc' : 'desc' };
}

export const PAGE_SIZE = 25;
/** The server serves offsets up to 9975. */
export const MAX_OFFSET = 9975;

export interface FilterOption {
  /** Row label. */
  label: string;
  /** Text that replaces the token being typed. */
  insert: string;
  hint?: string;
}

const FILTERS: { key: string; hint: string }[] = [
  { key: 'from:', hint: 'user' },
  { key: 'mentions:', hint: 'user' },
  { key: 'has:', hint: 'link, embed, file, image, video, sound, sticker, poll' },
  { key: 'before:', hint: 'specific date' },
  { key: 'during:', hint: 'specific date' },
  { key: 'after:', hint: 'specific date' },
  { key: 'in:', hint: 'channel' },
  { key: 'pinned:', hint: 'true or false' },
  { key: 'authorType:', hint: 'user, bot' },
];

export const HAS_VALUES = ['link', 'embed', 'file', 'image', 'video', 'sound', 'sticker', 'poll'];

const pad = (n: number) => String(n).padStart(2, '0');
export const isoDay = (d: Date) =>
  `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** The whitespace-delimited token the caret is in (the end of the text, since the box is a single line). */
export function currentToken(text: string): { token: string; start: number } {
  const start = text.lastIndexOf(' ') + 1;
  return { token: text.slice(start), start };
}

export interface FilterLookups {
  users: { id: string; names: string[] }[];
  channels: { id: string; name: string }[];
}

/**
 * Options for the token under construction: all filter names for a bare prefix, then values for
 * `from:`/`mentions:` (people), `in:` (channels), `has:`, dates and `pinned:`.
 */
export function filterOptions(
  token: string,
  lookups: FilterLookups,
  opts: { isDm: boolean; now?: Date } = { isDm: false },
): FilterOption[] {
  const now = opts.now ?? new Date();
  const m = /^(\w+):(.*)$/.exec(token);
  if (!m) {
    if (token === '' || token.length > 12) return [];
    const q = token.toLowerCase();
    return FILTERS.filter((f) => !(opts.isDm && f.key === 'in:'))
      .filter((f) => f.key.toLowerCase().startsWith(q))
      .map((f) => ({ label: f.key, insert: f.key, hint: f.hint }));
  }
  const key = m[1]!.toLowerCase();
  const val = m[2]!.replace(/^["@#]/, '').toLowerCase();
  const prefix = `${m[1]}:`;
  switch (key) {
    case 'from':
    case 'mentions':
      return lookups.users
        .filter((u) => u.names.some((n) => n.toLowerCase().includes(val)))
        .slice(0, 6)
        .map((u) => ({
          label: u.names[1] || u.names[0] || u.id,
          insert: `${prefix}${u.names[0] ?? u.id} `,
          hint: u.names[0],
        }));
    case 'in':
      return opts.isDm
        ? []
        : lookups.channels
            .filter((c) => c.name.toLowerCase().includes(val))
            .slice(0, 6)
            .map((c) => ({ label: `#${c.name}`, insert: `${prefix}${c.name} ` }));
    case 'has':
      return HAS_VALUES.filter((v) => v.startsWith(val)).map((v) => ({
        label: v,
        insert: `${prefix}${v} `,
      }));
    case 'pinned':
      return ['true', 'false']
        .filter((v) => v.startsWith(val))
        .map((v) => ({
          label: v,
          insert: `${prefix}${v} `,
        }));
    case 'before':
    case 'during':
    case 'after': {
      const yesterday = new Date(now.getTime() - 86_400_000);
      const week = new Date(now.getTime() - 7 * 86_400_000);
      return [
        { label: 'Today', insert: `${prefix}${isoDay(now)} `, hint: isoDay(now) },
        { label: 'Yesterday', insert: `${prefix}${isoDay(yesterday)} `, hint: isoDay(yesterday) },
        { label: 'A week ago', insert: `${prefix}${isoDay(week)} `, hint: isoDay(week) },
      ];
    }
    default:
      return [];
  }
}

/** Replaces the token being typed with an option. */
export function applyFilterOption(text: string, option: FilterOption): string {
  const { start } = currentToken(text);
  return text.slice(0, start) + option.insert;
}

export interface HitGroup<T> {
  channelId: string;
  hits: T[];
}

/** Groups hits by channel in first-seen order (the server order inside a channel is kept). */
export function groupByChannel<T extends { channel_id: string }>(
  hits: readonly T[],
): HitGroup<T>[] {
  const out: HitGroup<T>[] = [];
  const index = new Map<string, HitGroup<T>>();
  for (const h of hits) {
    let g = index.get(h.channel_id);
    if (!g) {
      g = { channelId: h.channel_id, hits: [] };
      index.set(h.channel_id, g);
      out.push(g);
    }
    g.hits.push(h);
  }
  return out;
}

const HISTORY_KEY = 'sr.lounge.searchHistory';
const HISTORY_MAX = 10;

export const searchHistory = {
  list(): string[] {
    try {
      const v: unknown = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]');
      return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
    } catch {
      return [];
    }
  },
  add(query: string): string[] {
    const q = query.trim();
    if (!q) return this.list();
    const next = [q, ...this.list().filter((x) => x !== q)].slice(0, HISTORY_MAX);
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
    } catch {
      /* history is a convenience */
    }
    return next;
  },
  clear() {
    try {
      localStorage.removeItem(HISTORY_KEY);
    } catch {
      /* ignore */
    }
  },
};
