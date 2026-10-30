// Typed fetchers for the scrapbook routes (crates/sr-api/src/routes/guilds/scrapbook.rs). Components never build URLs.
import { apiFetch, apiPost } from '../../api/client';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface ScrapbookObject {
  id: string;
  filename: string;
  content_type: string;
  size: number;
  kind: 'image' | 'video' | 'audio' | 'file';
  /** The viewer's storage scopes reach the file. False reads "not shared with you". */
  playable: boolean;
}

export interface ScrapbookEntry {
  id: string;
  channel_id: string;
  message_id: string;
  author_id: string;
  author_name: string;
  content: string;
  /** When the message was sent, unix ms. */
  sent_at: number;
  tags: string[];
  via: 'reaction' | 'manual';
  archived_by?: string;
  skipped: number;
  objects: ScrapbookObject[];
  can_edit: boolean;
}

export interface ScrapbookFacets {
  tags: { tag: string; count: number }[];
  authors: { id: string; name: string; count: number }[];
  channels: { id: string; count: number }[];
}

export interface ScrapbookPage {
  entries: ScrapbookEntry[];
  next_before?: string;
  facets?: ScrapbookFacets;
}

export interface ScrapbookTarget {
  mount_id: string;
  label: string;
  granted_by: string;
  ok: boolean;
  problem?: string;
}

export interface ScrapbookStatus {
  /** The operator has made the module available. False means nothing else is set. */
  offered: boolean;
  enabled: boolean;
  emojis: string[];
  threshold: number;
  can_send: boolean;
  can_manage: boolean;
  target?: ScrapbookTarget;
  failed_removals: number;
  ignored_channels: string[];
}

export interface ScrapbookRemoval {
  id: string;
  mount_id: string;
  key: string;
  message_id?: string;
  attempts: number;
  last_error: string;
  next_attempt_at: number;
  queued_at: number;
}

export interface GalleryFilter {
  channelId?: string;
  authorId?: string;
  tag?: string;
  /** `YYYY-MM-DD`. */
  from?: string;
  to?: string;
}

const base = (guildId: string) => `/guilds/${enc(guildId)}/scrapbook`;

export const getStatus = (guildId: string) =>
  apiFetch<ScrapbookStatus>(`${base(guildId)}/status`, NO_RETRY);

export function getGallery(guildId: string, f: GalleryFilter, before?: string, facets = false) {
  const q = new URLSearchParams();
  if (before) q.set('before', before);
  if (f.channelId) q.set('channel_id', f.channelId);
  if (f.authorId) q.set('author_id', f.authorId);
  if (f.tag) q.set('tag', f.tag);
  if (f.from) q.set('from', f.from);
  if (f.to) q.set('to', f.to);
  if (facets) q.set('facets', 'true');
  const qs = q.toString();
  return apiFetch<ScrapbookPage>(`${base(guildId)}${qs ? `?${qs}` : ''}`, NO_RETRY);
}

export const sendToScrapbook = (channelId: string, messageId: string, tags: string[] = []) =>
  apiFetch<ScrapbookEntry>(`/channels/${enc(channelId)}/messages/${enc(messageId)}/scrapbook`, {
    method: 'PUT',
    json: { tags },
    ...NO_RETRY,
  });

export const setTags = (guildId: string, entryId: string, tags: string[]) =>
  apiFetch<ScrapbookEntry>(`${base(guildId)}/${enc(entryId)}`, {
    method: 'PATCH',
    json: { tags },
    ...NO_RETRY,
  });

export const removeEntry = (guildId: string, entryId: string) =>
  apiFetch<{ copies_removed: number; copies_failed: number }>(`${base(guildId)}/${enc(entryId)}`, {
    method: 'DELETE',
    ...NO_RETRY,
  });

/** A presigned GET for one copy; the URL is used at once and never stored. */
export const presignObject = (guildId: string, entryId: string, objectId: string) =>
  apiPost<{ url: string; expires_at: string }>(
    `${base(guildId)}/${enc(entryId)}/objects/${enc(objectId)}/presign`,
  );

export const setTarget = (guildId: string, mountId: string) =>
  apiFetch<ScrapbookTarget>(`${base(guildId)}/target`, {
    method: 'PUT',
    json: { mount_id: mountId },
    ...NO_RETRY,
  });

export const clearTarget = (guildId: string) =>
  apiFetch<void>(`${base(guildId)}/target`, { method: 'DELETE', ...NO_RETRY });

export const listRemovals = (guildId: string) =>
  apiFetch<{ removals: ScrapbookRemoval[] }>(`${base(guildId)}/removals`, NO_RETRY);

export const retryRemoval = (guildId: string, id: string) =>
  apiPost<{ removed: boolean }>(`${base(guildId)}/removals/${enc(id)}/retry`);

export const dismissRemoval = (guildId: string, id: string) =>
  apiFetch<void>(`${base(guildId)}/removals/${enc(id)}`, { method: 'DELETE', ...NO_RETRY });

/** A mount of the server, from `GET /guilds/{id}/storage/mounts`, for the shelf picker. */
export interface GuildMount {
  id: string;
  label: string;
  status?: string;
}

export const listGuildMounts = (guildId: string) =>
  apiFetch<GuildMount[] | { mounts: GuildMount[] }>(
    `/guilds/${enc(guildId)}/storage/mounts`,
    NO_RETRY,
  ).then((r) => (Array.isArray(r) ? r : (r.mounts ?? [])));
