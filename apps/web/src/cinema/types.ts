// Local types for the Cinema HTTP API (docs/specs/cinema-media.md sections 8: catalog by p6-cinema-data, social
// and progress by p6-cinema-social). These are the shapes the UI works with; api.ts adapts the wire shapes to them
// so components never see the server's field names. Swap for @sideroom/api-client types after the wiring merge.

export type TitleKind = 'movie' | 'series' | 'anime';

export interface Title {
  id: string;
  kind: TitleKind;
  title: string;
  original_title?: string | null;
  year: number | null;
  runtime_min?: number | null;
  season_count?: number | null;
  content_rating?: string | null;
  genres: string[];
  synopsis: string;
  poster_url?: string | null;
  backdrop_url?: string | null;
  logo_url?: string | null;
  /** A progressive trailer file, when the server hands out one. */
  trailer_url?: string | null;
  /** The catalog has at least one trailer (playback goes through the player). */
  has_trailer?: boolean;
  /** Shown to everyone with an 18+ badge; opening needs the age gate (D-184). */
  age_restricted: boolean;
  /** Row extras: progress of the item in a continue-watching style row. */
  my?: MyState | null;
}

export interface MyState {
  in_list?: boolean;
  watched?: boolean;
  rating?: number | null;
  /** 0..100 progress of the title (movie) or of the furthest started episode (series). */
  progress_percent?: number | null;
  position_s?: number | null;
  /** Episode to resume, for series. */
  episode_id?: string | null;
}

export interface Person {
  name: string;
  role?: string | null;
}

export interface Variant {
  id: string;
  label: string;
  notes?: string | null;
  is_default?: boolean;
}

export interface Contributor {
  user_id: string;
  name: string;
  role: string;
}

export interface Season {
  id: string;
  number: number;
  title?: string | null;
  episode_count: number;
}

export interface TitleDetail {
  title: Title;
  cast: Person[];
  crew: Person[];
  studios: string[];
  audio_languages: string[];
  subtitle_languages: string[];
  variants: Variant[];
  seasons: Season[];
  /** Contribution credits (CIN-SOC-003) are not built server-side yet: always empty. */
  contributors: Contributor[];
  /** Patron downloads (CIN-DL) are not built yet. */
  patron_download: boolean;
}

export interface Episode {
  id: string;
  season: number;
  number: number;
  title: string;
  synopsis?: string | null;
  runtime_min?: number | null;
  still_url?: string | null;
  progress_percent?: number | null;
  watched: boolean;
}

export interface Row {
  /** Wire key, e.g. `anime-room`, `animation-banner`, `trending`, `continue-watching`. */
  key: string;
  source: 'curated' | 'generated' | 'provided';
  display: 'row' | 'hero' | 'banner';
  title: string;
  /** Italic second line, e.g. "Without limits.". */
  accent?: string | null;
  /** Small label next to the heading, e.g. "Selected for the room". */
  label?: string | null;
  /** Where "Explore all" goes. */
  explore_href?: string | null;
  titles: Title[];
}

export interface Discover {
  hero: Title[];
  rows: Row[];
}

export interface TitlePage {
  items: Title[];
  total: number;
  next_cursor?: string | null;
}

export interface Collection {
  id: string;
  title: string;
}

export type SortKey = 'updated' | 'newest' | 'title' | 'year' | 'runtime';
export type AgeFilter = 'include' | 'hide' | 'only';
export type TrendingScope = 'friends' | 'known' | 'global';

export interface BrowseParams {
  kind?: TitleKind;
  genres?: string;
  collection?: string;
  year_from?: number;
  year_to?: number;
  /** Content ratings, comma separated ("PG,R"): any of them. */
  rating?: string;
  runtime_max?: number;
  language?: string;
  subtitle_language?: string;
  age?: AgeFilter;
  sort?: SortKey;
  order?: 'asc' | 'desc';
  limit?: number;
  cursor?: string;
}

export interface Facet {
  name: string;
  count: number;
}

export interface Facets {
  genres: Facet[];
  ratings: Facet[];
  languages: Facet[];
  subtitle_languages: Facet[];
}

/** The caller's own state, gathered from the saved-list and continue-watching routes. */
export interface MyCinema {
  inList: Set<string>;
  resume: Map<string, { percent: number; position_s?: number; episode_id?: string | null }>;
}

export interface Review {
  id: string;
  author_id: string;
  rating?: number | null;
  body: string;
  spoiler: boolean;
  helpful_count: number;
  voted: boolean;
  mine: boolean;
  created_at_ms: number;
}

export interface Comment {
  id: string;
  parent_id?: string | null;
  author_id: string;
  body: string;
  spoiler: boolean;
  like_count: number;
  reply_count: number;
  liked: boolean;
  mine: boolean;
  created_at_ms: number;
}

export interface RatingSummary {
  mine: number | null;
  global: { average: number | null; count: number };
}
