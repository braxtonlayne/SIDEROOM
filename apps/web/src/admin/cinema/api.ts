// REST helpers for the Cinema publisher tools: /api/v1/admin/cinema/* (docs/specs/cinema-media.md section 8.5,
// crates/sr-api/src/routes/cinema/admin.rs). Ids are Snowflake strings, timestamps Unix milliseconds.
import { apiFetch } from '../../api/client';
import { putFile } from '../../lounge/msg/upload';

const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });
const get = <T>(path: string) => apiFetch<T>(path, { retries: 0 });
const enc = encodeURIComponent;

const qs = (p: Record<string, string | number | boolean | null | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(p))
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : '';
};

export type TitleKind = 'movie' | 'series' | 'anime';
export const TITLE_KINDS: TitleKind[] = ['movie', 'series', 'anime'];
export type MarkerKind = 'intro' | 'credits' | 'recap';
export type TrailerKind = 'trailer' | 'teaser' | 'clip';
export type CollectionDisplay = 'row' | 'hero' | 'banner';
export type Surface = 'discover' | 'movies' | 'series' | 'anime';
export const SURFACES: Surface[] = ['discover', 'movies', 'series', 'anime'];
export type ArtworkSlot = 'poster' | 'backdrop' | 'logo' | 'still' | 'season-poster';

export interface Title {
  id: string;
  kind: TitleKind;
  title: string;
  original_title?: string | null;
  alt_titles: string[];
  year?: number | null;
  runtime_min?: number | null;
  content_rating?: string | null;
  synopsis: string;
  tagline?: string | null;
  genres: string[];
  tags: string[];
  languages: string[];
  subtitle_languages: string[];
  cast: string[];
  directors: string[];
  writers: string[];
  studios: string[];
  crew: string[];
  age_restricted: boolean;
  poster_url?: string | null;
  backdrop_url?: string | null;
  logo_url?: string | null;
  external_ids: Record<string, string>;
  season_count: number;
  episode_count: number;
  has_trailer: boolean;
  playable: boolean;
  published: boolean;
  published_at_ms?: number | null;
  asset_id?: string | null;
  created_at_ms: number;
  updated_at_ms: number;
}

/** The editable fields (`PUT` replaces them all, so an omitted artwork key clears the artwork). */
export interface TitleInput {
  kind: TitleKind;
  title: string;
  original_title?: string | null;
  alt_titles?: string[];
  year?: number | null;
  runtime_min?: number | null;
  content_rating?: string | null;
  synopsis?: string;
  tagline?: string | null;
  genres?: string[];
  tags?: string[];
  languages?: string[];
  subtitle_languages?: string[];
  cast?: string[];
  directors?: string[];
  writers?: string[];
  studios?: string[];
  crew?: string[];
  age_restricted?: boolean;
  poster_key?: string | null;
  backdrop_key?: string | null;
  logo_key?: string | null;
  external_ids?: Record<string, string>;
}

export interface Season {
  id: string;
  title_id: string;
  number: number;
  name?: string | null;
  synopsis?: string | null;
  poster_url?: string | null;
  episode_count: number;
}
export interface SeasonInput {
  number: number;
  name?: string | null;
  synopsis?: string | null;
  poster_key?: string | null;
}
export interface Episode {
  id: string;
  title_id: string;
  season_id: string;
  season_number: number;
  number: number;
  absolute_number?: number | null;
  title: string;
  synopsis: string;
  air_date?: string | null;
  runtime_min?: number | null;
  still_url?: string | null;
  playable: boolean;
  published: boolean;
  published_at_ms?: number | null;
  asset_id?: string | null;
}
export interface EpisodeInput {
  number: number;
  absolute_number?: number | null;
  title?: string;
  synopsis?: string;
  air_date?: string | null;
  runtime_min?: number | null;
  still_key?: string | null;
}
export interface Variant {
  id: string;
  title_id: string;
  label: string;
  notes?: string | null;
  runtime_min?: number | null;
  playable: boolean;
  position: number;
  asset_id?: string | null;
}
export interface VariantInput {
  label: string;
  notes?: string | null;
  runtime_min?: number | null;
  asset_id?: string | null;
  position?: number | null;
}
export interface Trailer {
  id: string;
  title_id: string;
  label: string;
  kind: TrailerKind;
  language?: string | null;
  playable: boolean;
  age_gated: boolean;
  asset_id?: string | null;
}
export interface TrailerInput {
  label: string;
  kind?: TrailerKind;
  language?: string | null;
  asset_id?: string | null;
}
export interface Marker {
  id?: string | null;
  kind: MarkerKind;
  start_ms: number;
  end_ms: number;
  source?: string | null;
}
export interface MarkerInput {
  kind: MarkerKind;
  start_ms: number;
  end_ms: number;
}
export interface TitlePage {
  items: Title[];
  total: number;
  next_cursor?: string | null;
}
export interface AdminTitleDetail {
  title: Title;
  seasons: Season[];
  episodes: Episode[];
  variants: Variant[];
  trailers: Trailer[];
  markers: Marker[];
}
export interface Collection {
  id: string;
  slug: string;
  name: string;
  eyebrow?: string | null;
  blurb?: string | null;
  display: CollectionDisplay;
  surfaces: Surface[];
  position: number;
  published: boolean;
  item_count: number;
}
export interface CollectionInput {
  slug: string;
  name: string;
  eyebrow?: string | null;
  blurb?: string | null;
  display?: CollectionDisplay;
  surfaces?: Surface[];
  position?: number;
  published?: boolean;
}
export interface CollectionDetail {
  collection: Collection;
  items: Title[];
}
export interface ArtworkUpload {
  key: string;
  upload_url: string;
  content_type: string;
  expires_in_secs: number;
}

// ---- titles ----

export interface TitleQuery {
  kind?: string;
  published?: string;
  title?: string;
  age?: string;
  sort?: string;
  order?: string;
  limit?: number;
  offset?: number;
}
export const listTitles = (p: TitleQuery) => get<TitlePage>(`/admin/cinema/titles${qs({ ...p })}`);
export const getTitle = (id: string) => get<AdminTitleDetail>(`/admin/cinema/titles/${enc(id)}`);
export const createTitle = (b: TitleInput) => send<Title>('/admin/cinema/titles', 'POST', b);
export const updateTitle = (id: string, b: TitleInput) =>
  send<Title>(`/admin/cinema/titles/${enc(id)}`, 'PUT', b);
export const deleteTitle = (id: string) => send<void>(`/admin/cinema/titles/${enc(id)}`, 'DELETE');
export const publishTitle = (id: string) =>
  send<Title>(`/admin/cinema/titles/${enc(id)}/publish`, 'POST');
export const unpublishTitle = (id: string) =>
  send<Title>(`/admin/cinema/titles/${enc(id)}/unpublish`, 'POST');
export const mergeTitle = (id: string, targetId: string) =>
  send<Title>(`/admin/cinema/titles/${enc(id)}/merge`, 'POST', { target_id: targetId });
export const linkTitleAsset = (id: string, assetId: string | null) =>
  send<Title>(`/admin/cinema/titles/${enc(id)}/asset`, 'PUT', { asset_id: assetId });

// ---- artwork ----

export const reserveArtwork = (titleId: string, slot: ArtworkSlot, contentType: string) =>
  send<ArtworkUpload>(`/admin/cinema/titles/${enc(titleId)}/artwork`, 'POST', {
    slot,
    content_type: contentType,
  });

export const ARTWORK_TYPES = ['image/webp', 'image/jpeg', 'image/png'];

/** Reserve a slot, PUT the file to the presigned URL, and resolve with the object key to save on the title. */
export async function uploadArtwork(
  titleId: string,
  slot: ArtworkSlot,
  file: File,
  onProgress: (fraction: number) => void = () => undefined,
): Promise<string> {
  const r = await reserveArtwork(titleId, slot, file.type);
  await putFile(r.upload_url, file, onProgress, new AbortController().signal);
  return r.key;
}

/**
 * The storage key behind a signed artwork URL (`/cdn/<key>?ex=&sig=`). The API returns only signed URLs, and a
 * metadata `PUT` replaces the artwork keys, so the editor sends the existing keys back to keep the artwork.
 */
export function keyFromUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const path = new URL(url, 'http://local.invalid').pathname;
    const at = path.indexOf('/cdn/');
    if (at < 0) return null;
    const key = decodeURIComponent(path.slice(at + 5));
    return key.startsWith('cinema/') ? key : null;
  } catch {
    return null;
  }
}

// ---- seasons, episodes ----

export const createSeason = (titleId: string, b: SeasonInput) =>
  send<Season>(`/admin/cinema/titles/${enc(titleId)}/seasons`, 'POST', b);
export const updateSeason = (id: string, b: SeasonInput) =>
  send<Season>(`/admin/cinema/seasons/${enc(id)}`, 'PUT', b);
export const deleteSeason = (id: string) =>
  send<void>(`/admin/cinema/seasons/${enc(id)}`, 'DELETE');
export const createEpisode = (seasonId: string, b: EpisodeInput) =>
  send<Episode>(`/admin/cinema/seasons/${enc(seasonId)}/episodes`, 'POST', b);
export const updateEpisode = (id: string, b: EpisodeInput) =>
  send<Episode>(`/admin/cinema/episodes/${enc(id)}`, 'PUT', b);
export const deleteEpisode = (id: string) =>
  send<void>(`/admin/cinema/episodes/${enc(id)}`, 'DELETE');
export const publishEpisode = (id: string) =>
  send<Episode>(`/admin/cinema/episodes/${enc(id)}/publish`, 'POST');
export const unpublishEpisode = (id: string) =>
  send<Episode>(`/admin/cinema/episodes/${enc(id)}/unpublish`, 'POST');
export const linkEpisodeAsset = (id: string, assetId: string | null) =>
  send<Episode>(`/admin/cinema/episodes/${enc(id)}/asset`, 'PUT', { asset_id: assetId });

// ---- variants, trailers ----

export const createVariant = (titleId: string, b: VariantInput) =>
  send<Variant>(`/admin/cinema/titles/${enc(titleId)}/variants`, 'POST', b);
export const updateVariant = (id: string, b: VariantInput) =>
  send<Variant>(`/admin/cinema/variants/${enc(id)}`, 'PUT', b);
export const deleteVariant = (id: string) =>
  send<void>(`/admin/cinema/variants/${enc(id)}`, 'DELETE');
export const createTrailer = (titleId: string, b: TrailerInput) =>
  send<Trailer>(`/admin/cinema/titles/${enc(titleId)}/trailers`, 'POST', b);
export const updateTrailer = (id: string, b: TrailerInput) =>
  send<Trailer>(`/admin/cinema/trailers/${enc(id)}`, 'PUT', b);
export const deleteTrailer = (id: string) =>
  send<void>(`/admin/cinema/trailers/${enc(id)}`, 'DELETE');

// ---- markers ----

export const getEpisodeMarkers = (episodeId: string) =>
  get<{ markers: Marker[] }>(`/admin/cinema/episodes/${enc(episodeId)}/markers`);
export const putTitleMarkers = (titleId: string, markers: MarkerInput[]) =>
  send<{ markers: Marker[] }>(`/admin/cinema/titles/${enc(titleId)}/markers`, 'PUT', { markers });
export const putEpisodeMarkers = (episodeId: string, markers: MarkerInput[]) =>
  send<{ markers: Marker[] }>(`/admin/cinema/episodes/${enc(episodeId)}/markers`, 'PUT', {
    markers,
  });

// ---- collections ----

export const listCollections = () => get<{ items: Collection[] }>('/admin/cinema/collections');
export const getCollection = (id: string) =>
  get<CollectionDetail>(`/admin/cinema/collections/${enc(id)}`);
export const createCollection = (b: CollectionInput) =>
  send<Collection>('/admin/cinema/collections', 'POST', b);
export const updateCollection = (id: string, b: CollectionInput) =>
  send<Collection>(`/admin/cinema/collections/${enc(id)}`, 'PUT', b);
export const deleteCollection = (id: string) =>
  send<void>(`/admin/cinema/collections/${enc(id)}`, 'DELETE');
export const setCollectionItems = (id: string, titleIds: string[]) =>
  send<CollectionDetail>(`/admin/cinema/collections/${enc(id)}/items`, 'PUT', {
    title_ids: titleIds,
  });
