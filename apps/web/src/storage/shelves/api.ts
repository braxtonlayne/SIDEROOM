import { apiFetch } from '../../api/client';
import { presignGet, type Presigned } from '../api';

// Read and edit side of BYOS-CAT-002; shapes checked against the OpenAPI (ShelfListView, ShelfDetailView, ...).

export type ShelfSection = 'cinema' | 'library' | 'music';
export type ShelfScope = 'own' | 'friend' | 'guild' | 'staff';

/** How the viewer reaches a mount: the source chip. */
export interface ShelfSource {
  mount_id: string;
  label: string;
  scope: ShelfScope;
  owner_id?: string;
  guild_id?: string;
}

export interface ShelfItem {
  id: string;
  section: ShelfSection;
  kind: 'movie' | 'series' | 'anime' | 'manga' | 'comic' | 'book' | 'track';
  title: string;
  subtitle: string;
  year: number | null;
  source: ShelfSource;
  poster_url?: string;
  backdrop_url?: string;
  tags: string[];
  genres: string[];
  synopsis: string;
  /** The metadata provider's attribution; shown wherever fetched details are. */
  attribution?: string;
  external_ids: Record<string, string>;
  /** Fields the member edited; a re-index leaves them alone. */
  overridden: string[];
  /** Episodes or chapters. */
  parts: number;
  /** The file of a film, track or single book; mount-relative. */
  object_key?: string;
  container: string;
  /** Plays in a browser as is; otherwise greyed with "Convert to play". */
  web_native: boolean;
}

export interface ShelfPart {
  id: string;
  season?: number;
  number: number;
  title: string;
  object_key: string;
  container: string;
  web_native: boolean;
  size: number;
}

export interface ShelfList {
  items: ShelfItem[];
  total: number;
  /** The mounts the viewer reaches for this section: the "Shelves" chips. */
  sources: ShelfSource[];
}

export interface ShelfDetail {
  item: ShelfItem;
  parts: ShelfPart[];
  can_edit: boolean;
}

export interface ShelfQuery {
  q?: string;
  kind?: string;
  mount?: string;
  limit?: number;
  offset?: number;
}

export type ShelfProvider = 'tmdb' | 'anilist' | 'mangadex' | 'mangaupdates' | 'comicvine';

/** Body of `PATCH /shelves/{section}/{id}`: every field present is written and becomes an override. */
export interface ShelfPatch {
  title?: string;
  year?: number | null;
  kind?: string;
  tags?: string[];
  genres?: string[];
  synopsis?: string;
  poster_key?: string | null;
  backdrop_key?: string | null;
  artist?: string;
  album?: string;
  track_no?: number | null;
  /** Details fetched through `GET /submissions/lookup`; the server adds the provider's attribution text. */
  details?: { provider: ShelfProvider; external_id: string };
}

export interface PartPatch {
  title?: string;
  number?: number;
  season?: number;
}

export interface IndexStatus {
  queued: boolean;
  state: 'never' | 'idle' | 'running' | 'done' | 'failed';
  started_at?: string;
  finished_at?: string;
  objects: number;
  added: number;
  vanished: number;
  truncated: boolean;
  error?: string;
}

export const shelfKeys = {
  list: (section: ShelfSection, q: ShelfQuery) => ['shelves', section, q] as const,
  item: (section: ShelfSection, id: string) => ['shelves', section, 'item', id] as const,
  index: (mountId: string) => ['shelves', 'index', mountId] as const,
};

export const listShelf = (section: ShelfSection, q: ShelfQuery = {}) => {
  const p = new URLSearchParams();
  if (q.q) p.set('q', q.q);
  if (q.kind) p.set('kind', q.kind);
  if (q.mount) p.set('mount', q.mount);
  if (q.limit) p.set('limit', String(q.limit));
  if (q.offset) p.set('offset', String(q.offset));
  const qs = p.toString();
  return apiFetch<ShelfList>(`/shelves/${section}${qs ? `?${qs}` : ''}`, { retries: 0 });
};

export const getShelfItem = (section: ShelfSection, id: string) =>
  apiFetch<ShelfDetail>(`/shelves/${section}/${encodeURIComponent(id)}`, { retries: 0 });

export const patchShelfItem = (section: ShelfSection, id: string, body: ShelfPatch) =>
  apiFetch<ShelfDetail>(`/shelves/${section}/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

export const patchShelfPart = (section: ShelfSection, id: string, part: string, body: PartPatch) =>
  apiFetch<ShelfDetail>(
    `/shelves/${section}/${encodeURIComponent(id)}/parts/${encodeURIComponent(part)}`,
    { method: 'PATCH', json: body, retries: 0 },
  );

export const startIndex = (mountId: string) =>
  apiFetch<IndexStatus>(`/storage/mounts/${encodeURIComponent(mountId)}/index`, {
    method: 'POST',
    json: {},
    retries: 0,
  });

export const getIndexStatus = (mountId: string) =>
  apiFetch<IndexStatus>(`/storage/mounts/${encodeURIComponent(mountId)}/index`, { retries: 0 });

/** A presigned GET for playback or reading; the client drops it at `expires_at` and asks again. */
export const presignShelfFile = (mountId: string, key: string): Promise<Presigned> =>
  presignGet(mountId, key);

/** Containers a browser plays as is, by extension (the server's `web_native` is the authority). */
export const NATIVE_VIDEO = new Set(['mp4', 'm4v', 'webm']);

export type Playable = 'video' | 'audio' | 'cbz' | 'epub' | 'pdf' | 'none';

/** What opening a file does. A non-native video or audio file is `none`: "Convert to play" until wave 29. */
export function playableKind(container: string, webNative: boolean): Playable {
  const c = container.toLowerCase();
  if (c === 'cbz') return 'cbz';
  if (c === 'epub') return 'epub';
  if (c === 'pdf') return 'pdf';
  if (!webNative) return 'none';
  if (NATIVE_VIDEO.has(c)) return 'video';
  return 'audio';
}
