// Wire shapes of the personal Cinema routes (spec cinema-media section 8.1, 8.2, 8.5; p6-cinema-social). Every
// route is called from api.ts only.
export type CinemaKind = 'movie' | 'series' | 'anime';

/** A title as the social routes return it (resolved from the catalog at read time). */
export interface TitleRef {
  id: string;
  kind: CinemaKind;
  title: string;
  year?: number | null;
  age_restricted?: boolean;
  /** Signed poster link (`/cdn/cinema/...`); the cards fall back to a lettered tile without it. */
  poster_url?: string | null;
}

export interface EpisodeRef {
  id: string;
  title_id: string;
  season: number;
  number: number;
  title?: string | null;
  runtime_secs?: number | null;
}

export interface Progress {
  title_id: string;
  episode_id: string;
  position_secs: number;
  duration_secs: number;
  percent: number;
  completed: boolean;
  updated_at_ms: number;
}

/** `reason: "resume"` is an unfinished item (with `progress`); `"up_next"` carries the next episode instead. */
export interface ContinueItem {
  reason: 'resume' | 'up_next';
  title: TitleRef;
  episode?: EpisodeRef | null;
  progress?: Progress | null;
}

export type ListVisibility = 'private' | 'friends' | 'public';
export interface CinemaList {
  id: string;
  owner_id?: string;
  name: string;
  description?: string;
  kind: 'saved' | 'custom';
  visibility: ListVisibility;
  item_count: number;
}
export interface ListEntry {
  title: TitleRef;
  note?: string | null;
  added_at_ms: number;
}

export interface HistoryItem {
  title: TitleRef;
  episode?: EpisodeRef | null;
  progress: Progress;
}
