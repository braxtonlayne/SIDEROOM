// Pure helpers for the guide and the live player: time windows, cell geometry, filters, keyboard navigation.
import type { ChannelCategory, TvBlock, TvChannel } from './types';
import { formatDate, formatTime, t, type MessageKey } from '../../i18n';

export const MIN = 60_000;
export const HOUR = 60 * MIN;
export const DAY = 24 * HOUR;
/** One column of the time grid. */
export const SLOT_MS = 30 * MIN;
/** The window the arrows move by: four columns (spec: two-hour windows). */
export const WINDOW_MS = 2 * HOUR;

/** Local midnight of the day containing `ms`. */
export function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Start of the local day `n` days after `dayStart` (DST-safe, unlike adding 24 h). */
export function addDays(dayStart: number, n: number): number {
  const d = new Date(dayStart);
  d.setDate(d.getDate() + n);
  return d.getTime();
}

export function timeLabel(ms: number, tz?: string | null): string {
  try {
    return formatTime(ms, {
      hour: 'numeric',
      minute: '2-digit',
      ...(tz ? { timeZone: tz } : {}),
    });
  } catch {
    return formatTime(ms, { hour: 'numeric', minute: '2-digit', second: '2-digit' });
  }
}

export function dayLabel(dayStart: number, nowMs: number): string {
  const today = startOfDay(nowMs);
  if (dayStart === today) return t('cinema.tvModel.today');
  if (dayStart === addDays(today, 1)) return t('cinema.tvModel.tomorrow');
  if (dayStart === addDays(today, -1)) return t('cinema.tvModel.yesterday');
  return formatDate(dayStart, { weekday: 'short', month: 'short', day: 'numeric' });
}

/** "Morning", "Afternoon", "Evening" or "Late-night" lineup for the window that starts at `ms`. */
export function lineupName(ms: number): string {
  const h = new Date(ms).getHours();
  if (h >= 5 && h < 12) return t('cinema.tvModel.morningLineup');
  if (h >= 12 && h < 17) return t('cinema.tvModel.afternoonLineup');
  if (h >= 17 && h < 22) return t('cinema.tvModel.eveningLineup');
  return t('cinema.tvModel.lateNightLineup');
}

/** Pixel geometry of a block inside a track that starts at `trackStart`. */
export function cellBox(
  b: Pick<TvBlock, 'start_ms' | 'end_ms'>,
  trackStart: number,
  slotPx: number,
) {
  const left = ((Math.max(b.start_ms, trackStart) - trackStart) / SLOT_MS) * slotPx;
  const right = ((b.end_ms - trackStart) / SLOT_MS) * slotPx;
  return { left, width: Math.max(0, right - left) };
}

export const isNow = (b: Pick<TvBlock, 'start_ms' | 'end_ms'>, nowMs: number) =>
  b.start_ms <= nowMs && nowMs < b.end_ms;

export function nowAndNext(blocks: TvBlock[] | undefined, nowMs: number) {
  const list = blocks ?? [];
  const now = list.find((b) => isNow(b, nowMs)) ?? null;
  const next = list.find((b) => b.start_ms >= (now?.end_ms ?? nowMs)) ?? null;
  return { now, next };
}

export const runtimeMinutes = (b: Pick<TvBlock, 'start_ms' | 'end_ms'>) =>
  Math.max(1, Math.round((b.end_ms - b.start_ms) / MIN));

export function formatDuration(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h ? `${h}h${m ? ` ${m}m` : ''}` : `${m}m`;
}

const KIND_WORD: Record<TvBlock['kind'], MessageKey> = {
  movie: 'cinema.tvModel.kindMovie',
  series: 'cinema.tvModel.kindSeries',
  anime: 'cinema.tvModel.kindAnime',
  filler: 'cinema.tvModel.kindFiller',
};
/** "Anime · 1998". */
export const kindYear = (b: Pick<TvBlock, 'kind' | 'year'>) =>
  [t(KIND_WORD[b.kind]), b.year].filter(Boolean).join(' · ');

/* ------------------------------------------------------------------ filters */

export type TvFilter = 'all' | 'anime' | 'movies' | 'series' | 'custom' | 'favorites';
export const FILTERS: { value: TvFilter; label: string }[] = [
  // Labels resolve when read, so they follow the active language.
  {
    value: 'all',
    get label() {
      return t('cinema.tvModel.filterAll');
    },
  },
  {
    value: 'anime',
    get label() {
      return t('cinema.tvModel.filterAnime');
    },
  },
  {
    value: 'movies',
    get label() {
      return t('cinema.tvModel.filterMovies');
    },
  },
  {
    value: 'series',
    get label() {
      return t('cinema.tvModel.filterSeries');
    },
  },
  {
    value: 'custom',
    get label() {
      return t('cinema.tvModel.filterCustom');
    },
  },
  {
    value: 'favorites',
    get label() {
      return t('cinema.tvModel.filterFavorites');
    },
  },
];

/** The channel's category: the server's, else the kind of programme that dominates what it airs today. */
export function categoryOf(ch: TvChannel, blocks: TvBlock[] = []): ChannelCategory {
  if (ch.category) return ch.category;
  const count: Record<string, number> = {};
  for (const b of blocks) if (b.kind !== 'filler') count[b.kind] = (count[b.kind] ?? 0) + 1;
  const top = Object.entries(count).sort((a, b) => b[1] - a[1])[0]?.[0];
  if (top === 'anime') return 'anime';
  if (top === 'movie') return 'movies';
  if (top === 'series') return 'series';
  return 'custom';
}

/* ------------------------------------------------------------ guide order */

/**
 * Where a channel sorts in the guide (P29-TV-1): instance channels first, then each server's, then personal
 * ones, by number within each (numbers repeat across sources). The server's `GET /tv/channels` uses the same
 * order; the client sorts again because it merges and filters lists.
 */
export const channelOrder = (c: TvChannel): [number, string, number] => [
  c.pool?.kind === 'personal' ? 2 : c.pool?.kind === 'guild' ? 1 : 0,
  c.pool?.kind === 'guild' ? c.pool.guild_id : c.pool?.kind === 'personal' ? c.pool.user_id : '',
  c.number,
];

/** Snowflakes as numbers: a shorter decimal string is the smaller id. */
const compareIds = (a: string, b: string) =>
  a.length !== b.length ? a.length - b.length : a < b ? -1 : a > b ? 1 : 0;

/** Comparator for `channelOrder`. */
export function compareChannels(a: TvChannel, b: TvChannel): number {
  const [ra, ia, na] = channelOrder(a);
  const [rb, ib, nb] = channelOrder(b);
  return ra - rb || compareIds(ia, ib) || na - nb || compareIds(a.id, b.id);
}

export function filterChannels(
  channels: TvChannel[],
  blocks: Record<string, TvBlock[]>,
  filter: TvFilter,
  favourites: ReadonlySet<string>,
): TvChannel[] {
  const sorted = [...channels].sort(compareChannels);
  if (filter === 'all') return sorted;
  if (filter === 'favorites') return sorted.filter((c) => favourites.has(c.id));
  return sorted.filter((c) => categoryOf(c, blocks[c.id]) === filter);
}

/* --------------------------------------------------------- keyboard grid */

export type NavKey = 'ArrowLeft' | 'ArrowRight' | 'ArrowUp' | 'ArrowDown' | 'Home' | 'End';
export interface CellId {
  row: number;
  id: string;
}

/**
 * The cell to focus after a navigation key. Left/right walk the row; up/down pick, in the neighbouring row, the
 * block that covers the focused cell's start (or `anchorMs`), else the closest one by start time.
 */
export function neighbourCell(
  rows: TvBlock[][],
  from: CellId,
  key: NavKey,
  anchorMs?: number,
): CellId | null {
  const cur = rows[from.row];
  if (!cur) return null;
  const at = cur.findIndex((b) => b.id === from.id);
  if (at < 0) return null;
  if (key === 'ArrowLeft') return at > 0 ? { row: from.row, id: cur[at - 1]!.id } : null;
  if (key === 'ArrowRight')
    return at < cur.length - 1 ? { row: from.row, id: cur[at + 1]!.id } : null;
  if (key === 'Home') return cur[0] ? { row: from.row, id: cur[0].id } : null;
  if (key === 'End') return cur.length ? { row: from.row, id: cur[cur.length - 1]!.id } : null;
  const step = key === 'ArrowUp' ? -1 : 1;
  const target = anchorMs ?? cur[at]!.start_ms;
  for (let r = from.row + step; r >= 0 && r < rows.length; r += step) {
    const list = rows[r]!;
    if (list.length === 0) continue;
    const covering = list.find((b) => b.start_ms <= target && target < b.end_ms);
    if (covering) return { row: r, id: covering.id };
    const closest = [...list].sort(
      (a, b) => Math.abs(a.start_ms - target) - Math.abs(b.start_ms - target),
    )[0]!;
    return { row: r, id: closest.id };
  }
  return null;
}

/* ------------------------------------------------------------- hrefs */

export const tvHref = '/cinema/tv';
export const watchChannelHref = (channelId: string) =>
  `/cinema/tv/watch/${encodeURIComponent(channelId)}`;
export const titleHrefOf = (b: Pick<TvBlock, 'title_id'>) =>
  b.title_id ? `/cinema/t/${encodeURIComponent(b.title_id)}` : null;

/* ---------------------------------------------------------- live player */

/** Channel after `current` in guide order (`compareChannels`), wrapping. `dir` +1 is channel up. */
export function stepChannel(
  channels: TvChannel[],
  currentId: string,
  dir: 1 | -1,
): TvChannel | null {
  const list = [...channels].sort(compareChannels);
  if (list.length === 0) return null;
  const at = list.findIndex((c) => c.id === currentId);
  if (at < 0) return list[0]!;
  return list[(at + dir + list.length) % list.length]!;
}

/** Fraction (0..1) of the programme that has aired at `nowMs`. */
export function programProgress(b: Pick<TvBlock, 'start_ms' | 'end_ms'> | null, nowMs: number) {
  if (!b || b.end_ms <= b.start_ms) return 0;
  return Math.min(1, Math.max(0, (nowMs - b.start_ms) / (b.end_ms - b.start_ms)));
}

/** A DVR window is only offered when the playlist really keeps more than the usual sliding window. */
export const MIN_DVR_S = 90;
export const LIVE_EDGE_SLACK_S = 12;

export interface LiveWindow {
  start: number;
  end: number;
}
export const hasDvr = (w: LiveWindow | null) => !!w && w.end - w.start >= MIN_DVR_S;
export const behindLive = (w: LiveWindow | null, currentTime: number) =>
  w ? Math.max(0, w.end - currentTime) : 0;
export const atLiveEdge = (w: LiveWindow | null, currentTime: number) =>
  behindLive(w, currentTime) <= LIVE_EDGE_SLACK_S;
