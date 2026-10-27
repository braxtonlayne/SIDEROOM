// Typed fetchers for the Live TV viewer routes: docs/specs/cinema-live-tv.md section 8 ("HTTP API as built", crate
// `sr-tv`). The only place that knows URLs and wire field names: each fetcher adapts the server's shape to the UI
// types in types.ts. Not on the server (so the UI degrades honestly): block synopses and ratings (read from the Cinema catalog by title id, see useBlockInfo).
import { useQuery } from '@tanstack/react-query';
import { ApiError, apiFetch } from '../../api/client';
import { getTitle } from '../api';
import type {
  ChannelCategory,
  ChannelNow,
  TuneGrant,
  TvBlock,
  TvChannel,
  TvGuide,
  TvPool,
} from './types';

const enc = encodeURIComponent;
const BASE = '/tv';

export const tvKeys = {
  guide: (from: number, to: number) => ['cinema', 'tv', 'guide', from, to] as const,
  now: (id: string) => ['cinema', 'tv', 'now', id] as const,
  settings: () => ['cinema', 'tv', 'settings-blob'] as const,
  favourites: () => ['cinema', 'tv', 'favorites'] as const,
  reminders: () => ['cinema', 'tv', 'reminders'] as const,
  info: (titleId: string) => ['cinema', 'tv', 'title-info', titleId] as const,
};

/** True when the server has no Live TV routes (yet): 404/405/501 without an API error body. */
export const isNotAvailable = (e: unknown) =>
  e instanceof ApiError &&
  (e.status === 404 || e.status === 405 || e.status === 501) &&
  !e.body?.code;

/** The channel is off the air: nothing to tune (not an error of the request). */
export class OffAirError extends Error {
  constructor() {
    super('Channel is off air');
  }
}

/* ------------------------------------------------------------- wire shapes */

export interface WireBlock {
  id: string;
  channel_id: string;
  start_ms: number;
  end_ms: number;
  kind: 'program' | 'filler' | 'slate' | 'off_air';
  title: string;
  episode_title?: string | null;
  title_id?: string | null;
  episode_id?: string | null;
  media_kind?: 'movie' | 'series' | 'anime' | null;
  season_number?: number | null;
  episode_number?: number | null;
  year?: number | null;
  age_restricted?: boolean;
  playable?: boolean;
  source?: 'generated' | 'manual' | 'override';
}

export interface WireChannel {
  id: string;
  number: number;
  name: string;
  call_sign?: string | null;
  era_label?: string | null;
  theme?: string | null;
  logo_url?: string | null;
  kind: 'rule_based' | 'manual' | 'imported_guide';
  timezone: string;
  age_restricted: boolean;
  viewers?: number;
  pool?: TvPool | null;
}

/** "S1 · E3 · The Long Night", whatever of it the server knows. */
export function episodeLabel(
  b: Pick<WireBlock, 'season_number' | 'episode_number' | 'episode_title'>,
) {
  const parts = [
    b.season_number != null ? `S${b.season_number}` : null,
    b.episode_number != null ? `E${b.episode_number}` : null,
    b.episode_title || null,
  ].filter(Boolean);
  return parts.length ? parts.join(' · ') : null;
}

/** A server block as the UI's programme cell. Slates, bars and bumpers are "filler". */
export function toBlock(w: WireBlock): TvBlock {
  return {
    id: String(w.id),
    channel_id: String(w.channel_id),
    start_ms: w.start_ms,
    end_ms: w.end_ms,
    title: w.title,
    episode_label: episodeLabel(w),
    kind: w.kind === 'program' ? (w.media_kind ?? 'series') : 'filler',
    title_id: w.title_id ? String(w.title_id) : null,
    episode_id: w.episode_id ? String(w.episode_id) : null,
    year: w.year ?? null,
    age_restricted: !!w.age_restricted,
    playable: w.playable ?? (w.kind !== 'off_air' && w.kind !== 'slate'),
    source: w.source ?? 'generated',
  };
}

export function toChannel(w: WireChannel): TvChannel {
  const custom = w.kind === 'manual';
  return {
    id: String(w.id),
    number: w.number,
    call_sign: w.call_sign || w.name.toUpperCase(),
    name: w.name,
    // The sub-label under the call sign: the era, or CUSTOM for staff-placed channels.
    network: w.era_label || (custom ? 'CUSTOM' : null),
    era_label: w.era_label ?? null,
    logo_url: w.logo_url ?? null,
    kind: w.kind,
    category: custom ? ('custom' as ChannelCategory) : null,
    timezone: w.timezone,
    age_restricted: w.age_restricted,
    viewers: w.viewers ?? 0,
    pool: w.pool ?? null,
  };
}

interface WireGuide {
  now_ms?: number;
  channels?: { channel: WireChannel; blocks: WireBlock[] }[];
}

export function normaliseGuide(w: WireGuide, fallbackNow = Date.now()): TvGuide {
  const channels: TvChannel[] = [];
  const blocks: Record<string, TvBlock[]> = {};
  for (const entry of w.channels ?? []) {
    const ch = toChannel(entry.channel);
    channels.push(ch);
    blocks[ch.id] = (entry.blocks ?? []).map(toBlock).sort((a, b) => a.start_ms - b.start_ms);
  }
  return { now_ms: w.now_ms ?? fallbackNow, channels, blocks, footer_note: null };
}

/** `GET /tv/guide?from&to` (unix ms, a window of at most 48 h). */
export async function getGuide(from: number, to: number): Promise<TvGuide> {
  return normaliseGuide(
    await apiFetch<WireGuide>(`${BASE}/guide?from=${from}&to=${to}`, { retries: 0 }),
  );
}

interface WireNow {
  channel_id: string;
  at_ms: number;
  offset_ms?: number | null;
  current?: WireBlock | null;
  next?: WireBlock | null;
  viewers?: number;
}

/** `GET /tv/channels/{id}/now`. A channel is in good reception while a playable programme is on. */
export async function getChannelNow(id: string): Promise<ChannelNow> {
  const w = await apiFetch<WireNow>(`${BASE}/channels/${enc(id)}/now`, { retries: 0 });
  const now = w.current ? toBlock(w.current) : null;
  return {
    now,
    next: w.next ? toBlock(w.next) : null,
    healthy: !!now && now.playable !== false,
    server_time_ms: w.at_ms,
    offset_ms: w.offset_ms ?? null,
    viewers: w.viewers ?? 0,
  };
}

interface WireTune {
  channel_id: string;
  state: 'live' | 'off_air';
  master_url?: string | null;
  /** True when `master_url` is the instance slate loop played off air (D-266). */
  slate?: boolean;
  current?: WireBlock | null;
  next?: WireBlock | null;
  offset_ms?: number | null;
  viewers?: number;
}

/** Playlists are re-signed on every media playlist reload (6 h URLs); the master carries no signature. */
const MASTER_TTL_MS = 24 * 3_600_000;

/**
 * `GET /tv/channels/{id}/tune` plus the channel itself: the live master playlist (a session-authenticated API
 * path), what is on now and next. Also what the player calls to refresh. 403 code 900011 is the age gate (D-184);
 * an off-air channel throws `OffAirError` unless the server plays its slate (`slate`, D-266): then the grant carries
 * the slate master and the view draws the off-air captions over it.
 */
export async function tuneChannel(id: string): Promise<TuneGrant> {
  const [tune, channel] = await Promise.all([
    apiFetch<WireTune>(`${BASE}/channels/${enc(id)}/tune`, { retries: 0 }),
    apiFetch<WireChannel>(`${BASE}/channels/${enc(id)}`, { retries: 0 }),
  ]);
  if (!tune.master_url) throw new OffAirError();
  const now = Date.now();
  return {
    channel: toChannel(channel),
    master_url: tune.master_url,
    slate: tune.state === 'off_air' && tune.slate === true,
    expires_at_ms: now + MASTER_TTL_MS,
    refresh_at_ms: null,
    now: tune.current ? toBlock(tune.current) : null,
    next: tune.next ? toBlock(tune.next) : null,
    server_time_ms: now,
    dvr_window_s: null,
  };
}

/** A fresh grant for the player session (it asks by media id `tv:<channel>`). */
export async function refreshTune(channelId: string): Promise<{
  master_url: string;
  expires_at_ms: number;
  refresh_at_ms?: number | null;
}> {
  const g = await tuneChannel(channelId);
  return {
    master_url: g.master_url,
    expires_at_ms: g.expires_at_ms,
    refresh_at_ms: g.refresh_at_ms ?? null,
  };
}

/** `PUT|DELETE /tv/reminders/{blockId}` (TV-PLY-005, G-10): a DM and a push shortly before the programme starts. */
export const setReminder = (blockId: string, on: boolean) =>
  apiFetch<void>(`${BASE}/reminders/${enc(blockId)}`, {
    method: on ? 'PUT' : 'DELETE',
    retries: 0,
  });

/** One pending reminder of the caller (`GET /tv/reminders`, H-01). `title` is null once the programme left the guide. */
export interface TvReminder {
  block_id: string;
  channel_id: string;
  title: string | null;
  starts_at: number;
  remind_at: number;
  lead_minutes: number;
}

export const listReminders = async () =>
  (await apiFetch<{ items: TvReminder[] }>(`${BASE}/reminders`, { retries: 0 })).items;

/* ------------------------------------------- catalog details for a block */

/**
 * The server's block carries no synopsis, rating or artwork: they come from the Cinema catalog by title id, only
 * when a popover or the preview panel needs them. A failure leaves the block as it is.
 */
export function useBlockInfo(block: TvBlock | null): TvBlock | null {
  const id = block?.title_id ?? null;
  const q = useQuery({
    queryKey: tvKeys.info(id ?? ''),
    queryFn: async () => (await getTitle(id!)).title,
    enabled: !!id,
    staleTime: 10 * 60_000,
    retry: false,
  });
  if (!block) return null;
  const t = q.data;
  if (!t) return block;
  return {
    ...block,
    synopsis: block.synopsis ?? t.synopsis ?? null,
    content_rating: block.content_rating ?? t.content_rating ?? null,
    runtime_min: block.runtime_min ?? t.runtime_min ?? null,
    poster_url: block.poster_url ?? t.poster_url ?? null,
    backdrop_url: block.backdrop_url ?? t.backdrop_url ?? null,
    year: block.year ?? t.year ?? null,
  };
}
