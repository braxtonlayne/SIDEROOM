// Wire shapes of the member routes under /library (crates/sr-api/src/routes/library/member.rs; generated in
// packages/api-client/src/generated/schema.d.ts). Kept as local types so apps/web needs no new dependency.

export type SeriesKind = 'manga' | 'manhwa' | 'manhua' | 'webtoon' | 'comic' | 'novel' | 'book';
export type SeriesStatus = 'ongoing' | 'completed' | 'hiatus' | 'cancelled';
export type ReadingDirection = 'rtl' | 'ltr' | 'vertical';
export type AgeFilter = 'hide' | 'include' | 'only';
export type SortKey = 'newest' | 'updated' | 'title' | 'followers';
export type Shelf = 'reading' | 'plan_to_read' | 'completed' | 'on_hold' | 'dropped';
export type ChapterFormat = 'images' | 'pdf' | 'epub';

export interface Series {
  id: string;
  kind: SeriesKind;
  title: string;
  alt_titles: string[];
  authors: string[];
  artists: string[];
  status: SeriesStatus;
  synopsis: string;
  genres: string[];
  tags: string[];
  reading_direction: ReadingDirection;
  age_restricted: boolean;
  published: boolean;
  chapter_count: number;
  follower_count: number;
  cover_url?: string | null;
  year?: number | null;
  last_chapter_at_ms?: number | null;
  merged_into?: string | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface SeriesPage {
  items: Series[];
  total: number;
  limit: number;
  offset: number;
}

export interface Edition {
  id: string;
  series_id: string;
  language: string;
  label: string;
  is_default: boolean;
}

export interface Follow {
  shelf: Shelf;
  notify: boolean;
  created_at_ms: number;
}

export interface SeriesDetail {
  series: Series;
  editions: Edition[];
  follow?: Follow | null;
}

export interface ChapterProgress {
  page_index: number;
  percent: number;
  completed: boolean;
  updated_at_ms?: number;
}

export interface Chapter {
  id: string;
  series_id: string;
  edition_id: string;
  volume_id?: string | null;
  number: number;
  title?: string | null;
  page_count: number;
  format: ChapterFormat;
  release_group?: string | null;
  published: boolean;
  published_at_ms?: number | null;
  progress?: ChapterProgress | null;
}

export interface Volume {
  id: string;
  edition_id: string;
  number: number;
  title?: string | null;
}

export interface ChapterList {
  editions: Edition[];
  volumes: Volume[];
  chapters: Chapter[];
}

export interface PageVariant {
  width: number;
  url: string;
}

export interface PageEntry {
  index: number;
  width: number;
  height: number;
  is_double_spread: boolean;
  url: string;
  variants: PageVariant[];
  blurhash?: string | null;
}

export interface PageManifest {
  chapter: Chapter;
  reading_direction: ReadingDirection;
  pages: PageEntry[];
  /** PDF and EPUB chapters: a signed link to the original file, with no pages (library-reader spec 6.4). */
  source_url?: string | null;
  expires_at: number;
  next_chapter_id?: string | null;
  prev_chapter_id?: string | null;
}

export interface InstantSearch {
  series: Series[];
  authors: string[];
  genres: string[];
  tags: string[];
}

export interface UpdateItem {
  chapter_id: string;
  chapter_number: number;
  chapter_title?: string | null;
  published_at_ms: number;
  language: string;
  release_group?: string | null;
  series_id: string;
  series_title: string;
  kind: SeriesKind;
  age_restricted: boolean;
  cover_url?: string | null;
}

export interface UpdatesPage {
  items: UpdateItem[];
  next_before?: string | null;
}

export interface FollowedSeries {
  series: Series;
  follow: Follow;
}

export interface FollowPage {
  items: FollowedSeries[];
  total: number;
  limit: number;
  offset: number;
}

export interface BrowseParams {
  kind?: SeriesKind;
  status?: SeriesStatus;
  genres?: string[];
  exclude_genres?: string[];
  tags?: string[];
  language?: string;
  year_from?: number;
  year_to?: number;
  age?: AgeFilter;
  sort?: SortKey;
  order?: 'asc' | 'desc';
  limit?: number;
  offset?: number;
}
