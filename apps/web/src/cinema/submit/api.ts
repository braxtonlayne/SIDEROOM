// Typed fetchers for the member video submission routes (`/submissions/{id}/cinema/*`, RDY-MED-001,
// spec cinema-media section 8.7). The submission itself (create, submit, withdraw, comments) uses the
// shared review fetchers in `../../review/api`.
import { apiFetch } from '../../api/client';

const enc = encodeURIComponent;
const post = <T>(path: string, json?: unknown) =>
  apiFetch<T>(path, { method: 'POST', retries: 0, ...(json !== undefined ? { json } : {}) });
const put = <T>(path: string, json: unknown) =>
  apiFetch<T>(path, { method: 'PUT', retries: 0, json });

/** The states of a video's media asset, in processing order, then the dead ends. */
export type AssetState =
  | 'awaiting_upload'
  | 'uploaded'
  | 'probing'
  | 'proxy_ready'
  | 'in_review'
  | 'approved'
  | 'transcoding'
  | 'published'
  | 'failed'
  | 'rejected'
  | 'removed'
  | 'expired';

export interface DraftVideo {
  asset_id: string;
  state: AssetState;
  error: string | null;
  filename: string;
  size_bytes: number;
  duration_ms: number | null;
  has_proxy: boolean;
  playable: boolean;
}

export interface DraftItem {
  target_kind: 'title' | 'episode';
  target_id: string;
  season_number: number | null;
  number: number | null;
  label: string;
  video: DraftVideo | null;
}

export interface PendingUpload {
  video: DraftVideo;
  target_kind: 'title' | 'episode';
  target_id: string;
  part_size: number;
  part_count: number;
}

/** The title as the draft shows it (the fields the editor reads and writes). */
export interface DraftTitle {
  id: string;
  kind: 'movie' | 'series' | 'anime';
  title: string;
  original_title: string | null;
  alt_titles: string[];
  year: number | null;
  runtime_min: number | null;
  content_rating: string | null;
  synopsis: string;
  tagline: string | null;
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
  poster_url: string | null;
  backdrop_url: string | null;
  external_ids: Record<string, string>;
}

export interface DraftEpisode {
  id: string;
  season_number: number;
  number: number;
  title: string;
  synopsis: string;
  air_date: string | null;
  runtime_min: number | null;
  asset_id?: string;
  published: boolean;
}

export interface Draft {
  draft: {
    new_title: boolean;
    title: DraftTitle;
    seasons: { id: string; number: number; name: string | null }[];
    episodes: DraftEpisode[];
    published_episodes: number;
  };
  items: DraftItem[];
  pending: PendingUpload[];
}

export interface EpisodeBody {
  number: number;
  absolute_number?: number | null;
  title: string;
  synopsis: string;
  air_date?: string | null;
  runtime_min?: number | null;
}

export interface VideoSlot {
  item: DraftItem;
  created: boolean;
  asset_id: string;
  part_size: number;
  part_count: number;
  parts: { part_number: number; url: string }[];
  expires_in_secs: number;
  warnings: string[];
  resumable: boolean;
}

export interface UploadParts {
  part_size: number;
  part_count: number;
  received: { part_number: number; size: number }[];
  missing: { part_number: number; url: string }[];
  expires_in_secs: number;
}

const base = (id: string) => `/submissions/${enc(id)}/cinema`;

export const getDraft = (id: string) => apiFetch<Draft>(`${base(id)}/draft`);
export const updateTitle = (id: string, body: Record<string, unknown>) =>
  put<DraftTitle>(`${base(id)}/title`, body);
export const addEpisode = (id: string, body: EpisodeBody & { season?: number }) =>
  post<DraftEpisode>(`${base(id)}/episodes`, body);
export const updateEpisode = (id: string, episode: string, body: EpisodeBody) =>
  put<DraftEpisode>(`${base(id)}/episodes/${enc(episode)}`, body);
export const deleteEpisode = (id: string, episode: string) =>
  apiFetch<void>(`${base(id)}/episodes/${enc(episode)}`, { method: 'DELETE', retries: 0 });

export const beginFile = (
  id: string,
  body: {
    filename: string;
    size_bytes: number;
    season?: number;
    episode?: number;
    episode_id?: string;
  },
) => post<VideoSlot>(`${base(id)}/files`, body);
export const uploadStatus = (id: string, asset: string) =>
  apiFetch<DraftVideo>(`${base(id)}/uploads/${enc(asset)}`);
export const uploadParts = (id: string, asset: string) =>
  apiFetch<UploadParts>(`${base(id)}/uploads/${enc(asset)}/parts`);
export const completeUpload = (id: string, asset: string) =>
  post<DraftVideo>(`${base(id)}/uploads/${enc(asset)}/complete`);
export const abortUpload = (id: string, asset: string) =>
  apiFetch<void>(`${base(id)}/uploads/${enc(asset)}`, { method: 'DELETE', retries: 0 });
