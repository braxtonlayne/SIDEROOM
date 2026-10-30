// Typed fetchers for the status-ring routes (crates/sr-api/src/routes/userfeat/rings.rs, PRF-RNG-001). Components
// never build URLs. A ring the caller may not see is a plain 404 "Unknown ring", the same as one that is gone.
import { apiFetch } from '../../api/client';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export type RingKind = 'photo' | 'clip' | 'text' | 'track';
export type RingAudience = 'friends' | 'guilds';

export interface RingTrack {
  title: string;
  artist: string;
}

export interface Ring {
  id: string;
  author_id: string;
  kind: RingKind;
  body: string;
  track: RingTrack | null;
  has_media: boolean;
  content_type: string | null;
  duration_ms: number | null;
  audience: RingAudience;
  /** The chosen servers; filled for the author only. */
  guild_ids: string[];
  created_at: string;
  expires_at: string;
  /** The viewer opened it already (the author's own rings are always seen). */
  seen: boolean;
  mine: boolean;
}

export interface RingFeedEntry {
  author_id: string;
  all_seen: boolean;
  /** Oldest first, the order they play in. */
  rings: Ring[];
}

export interface RingViewerRow {
  user_id: string;
  viewed_at: string;
}

export interface NewTextOrTrackRing {
  kind: 'text' | 'track';
  text?: string;
  track?: RingTrack;
  audience: RingAudience;
  guildIds?: string[];
}

export interface NewFileRing {
  kind: 'photo' | 'clip';
  file: Blob;
  caption?: string;
  /** A clip's length in milliseconds. */
  durationMs?: number;
  audience: RingAudience;
  guildIds?: string[];
}

export const getRingFeed = () => apiFetch<RingFeedEntry[]>('/rings', NO_RETRY);

export const getMyRings = () => apiFetch<Ring[]>('/users/@me/rings', NO_RETRY);

export const postRing = (r: NewTextOrTrackRing) =>
  apiFetch<Ring>('/users/@me/rings', {
    method: 'POST',
    json: {
      kind: r.kind,
      text: r.text ?? '',
      ...(r.track ? { track: r.track } : {}),
      audience: r.audience,
      guild_ids: r.audience === 'guilds' ? (r.guildIds ?? []) : [],
    },
    ...NO_RETRY,
  });

export const uploadRing = (r: NewFileRing) => {
  const q = new URLSearchParams({ kind: r.kind, audience: r.audience });
  if (r.audience === 'guilds' && r.guildIds?.length) q.set('guild_ids', r.guildIds.join(','));
  if (r.caption) q.set('text', r.caption);
  if (r.kind === 'clip' && r.durationMs) q.set('duration_ms', String(Math.round(r.durationMs)));
  return apiFetch<Ring>(`/users/@me/rings/media?${q.toString()}`, {
    method: 'POST',
    raw: r.file,
    headers: { 'content-type': 'application/octet-stream' },
    ...NO_RETRY,
  });
};

export const deleteRing = (id: string) =>
  apiFetch<void>(`/users/@me/rings/${enc(id)}`, { method: 'DELETE', ...NO_RETRY });

/** The author's viewer list. Only the author gets one; anybody else gets 404. */
export const getRingViewers = (id: string) =>
  apiFetch<RingViewerRow[]>(`/users/@me/rings/${enc(id)}/viewers`, NO_RETRY);

export const markRingViewed = (id: string) =>
  apiFetch<void>(`/rings/${enc(id)}/view`, { method: 'POST', json: {}, ...NO_RETRY });

/** Where the photo or clip of a ring is served (the session cookie authorises it). */
export const ringMediaUrl = (id: string) => `/api/v1/rings/${enc(id)}/media`;
