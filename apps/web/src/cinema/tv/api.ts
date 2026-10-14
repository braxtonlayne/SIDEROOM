// Typed fetchers for the Live TV viewer routes (spec cinema-live-tv section 5). The only place that knows URLs
// and wire field names. The routes are the spec's (`GET /sr/tv/guide`, `GET /sr/tv/channels/{id}/now`) plus the
// ones the backend agent documents under "HTTP API as built"; names that were guesses when this was written
// are marked ASSUMED and are a one-file change here (plus the MSW fixtures in __tests__).
import { ApiError, apiFetch } from '../../api/client';
import type { ChannelNow, TuneGrant, TvBlock, TvChannel, TvGuide } from './types';

const enc = encodeURIComponent;
const BASE = '/sr/tv';

export const tvKeys = {
  guide: (from: number, to: number) => ['cinema', 'tv', 'guide', from, to] as const,
  now: (id: string) => ['cinema', 'tv', 'now', id] as const,
  channels: () => ['cinema', 'tv', 'channels'] as const,
  settings: () => ['cinema', 'tv', 'settings-blob'] as const,
};

/** True when the server has no Live TV routes (yet): 404/405/501 without an API error body. */
export const isNotAvailable = (e: unknown) =>
  e instanceof ApiError &&
  (e.status === 404 || e.status === 405 || e.status === 501) &&
  !e.body?.code;

type WireGuide = Partial<Omit<TvGuide, 'blocks'>> & {
  blocks?: Record<string, TvBlock[]> | TvBlock[];
  channels?: TvChannel[];
};

/** Accepts blocks grouped per channel or as one flat list, and always returns them sorted per channel. */
export function normaliseGuide(w: WireGuide, fallbackNow = Date.now()): TvGuide {
  const blocks: Record<string, TvBlock[]> = {};
  const add = (b: TvBlock) => (blocks[b.channel_id] ??= []).push(b);
  if (Array.isArray(w.blocks)) w.blocks.forEach(add);
  else
    for (const [id, list] of Object.entries(w.blocks ?? {}))
      for (const b of list) add({ ...b, channel_id: b.channel_id ?? id });
  for (const list of Object.values(blocks)) list.sort((a, b) => a.start_ms - b.start_ms);
  return {
    now_ms: w.now_ms ?? fallbackNow,
    channels: w.channels ?? [],
    blocks,
    footer_note: w.footer_note ?? null,
  };
}

/** `GET /sr/tv/guide?from&to` (unix ms). */
export async function getGuide(from: number, to: number): Promise<TvGuide> {
  const w = await apiFetch<WireGuide>(`${BASE}/guide?from=${from}&to=${to}`, { retries: 0 });
  return normaliseGuide(w);
}

/** `GET /sr/tv/channels/{id}/now`. */
export const getChannelNow = (id: string) =>
  apiFetch<ChannelNow>(`${BASE}/channels/${enc(id)}/now`, { retries: 0 });

/** ASSUMED `GET /sr/tv/channels`: the channel list on its own (the player's channel overlay). */
export async function getChannels(): Promise<TvChannel[]> {
  const r = await apiFetch<{ items?: TvChannel[]; channels?: TvChannel[] }>(`${BASE}/channels`, {
    retries: 0,
  });
  return r.items ?? r.channels ?? [];
}

/**
 * ASSUMED `POST /sr/tv/channels/{id}/tune`: a signed live master playlist for the channel plus what is on now and
 * next. Called on tuning and again to refresh the signature. 403 with code 900011 is the age gate (D-184).
 */
export const tuneChannel = (id: string) =>
  apiFetch<TuneGrant>(`${BASE}/channels/${enc(id)}/tune`, { method: 'POST', retries: 0 });

/** A fresh signature for the player session (it asks by media id `tv:<channel>`). */
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

/** ASSUMED `PUT /sr/tv/reminders/{blockId}` (TV-PLY-005). */
export const setReminder = (blockId: string, on: boolean) =>
  apiFetch<void>(`${BASE}/reminders/${enc(blockId)}`, {
    method: on ? 'PUT' : 'DELETE',
    retries: 0,
  });
