// Wire shapes the player assumes. The backend agents (p6-cinema-data/-social/-ingest) own the contract in
// docs/specs/cinema-media.md; everything the player knows about the API is in api.ts and progress.ts, so
// a difference is a one-file change. Kept as local types (no generated client yet for the Cinema routes).

export type MarkerKind = 'intro' | 'credits' | 'recap';
export interface Marker {
  kind: MarkerKind;
  start_s: number;
  end_s: number;
}

export interface AudioTrackInfo {
  id: string;
  language: string;
  label: string;
  is_default?: boolean;
}

export interface SubtitleTrackInfo {
  id: string;
  language: string;
  label: string;
  /** WebVTT, always present (ASS sources are converted, CIN-ING-008). */
  vtt_url: string;
  /** The styled original, rendered with an ASS renderer (JASSUB) when one is available. */
  ass_url?: string | null;
  is_default?: boolean;
  forced?: boolean;
}

export interface NextEpisode {
  title_id: string;
  episode_id: string;
  season: number;
  number: number;
  title: string;
  still_url?: string | null;
}

export interface WatchProgress {
  position_s: number;
  duration_s: number;
  completed: boolean;
  updated_at_ms?: number;
}

export type TitleKind = 'movie' | 'series' | 'anime';

/** `GET /cinema/titles/{id}/playback[?episode=]`: everything the player needs for one playable item. */
export interface PlaybackInfo {
  title_id: string;
  episode_id: string | null;
  /** Stable id of the playable asset; signed URL refresh is keyed by it. */
  media_id: string;
  title: string;
  /** "S1 · E3 · The Long Night" for episodes. */
  episode_label?: string | null;
  kind: TitleKind;
  poster_url?: string | null;
  age_restricted: boolean;
  duration_s: number;
  /** Signed HLS master playlist. */
  master_url: string;
  expires_at_ms: number;
  audio_tracks: AudioTrackInfo[];
  subtitle_tracks: SubtitleTrackInfo[];
  markers: Marker[];
  /** WebVTT sprite map (one frame every 10 s), null when none was generated. */
  sprites_url: string | null;
  next: NextEpisode | null;
  progress: WatchProgress | null;
}

/** `POST /cinema/media/{mediaId}/refresh` */
export interface RefreshedSource {
  master_url: string;
  expires_at_ms: number;
}

export interface SpriteCue {
  start: number;
  end: number;
  url: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/* ---- downloads (CIN-DL-002) ---- */
export interface DownloadRendition {
  id: string;
  label: string;
  height: number | null;
  size_bytes: number;
  /** `original` is only offered when staff enabled original downloads for the title. */
  kind: 'remux' | 'original';
  /** Already prepared and cached (7 days): no job needed. */
  cached?: boolean;
}
export interface DownloadOptions {
  renditions: DownloadRendition[];
  daily_remaining?: number | null;
}
export type DownloadState = 'queued' | 'preparing' | 'ready' | 'failed';
export interface DownloadJob {
  id: string;
  state: DownloadState;
  /** 0..1 */
  progress: number;
  url?: string | null;
  expires_at_ms?: number | null;
  error?: string | null;
}
