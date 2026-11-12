// URL search params of the Library browse page. Kept apart from Browse.tsx so the router can validate them without
// pulling the page (and the UI kit) into the first-paint chunk (SEC-AUD-144, as cinema/browseSearch.ts does).
import type { AgeFilter, SeriesKind, SeriesStatus, SortKey } from './types';

export interface BrowseSearch {
  q?: string;
  kind?: SeriesKind;
  status?: SeriesStatus;
  genres?: string;
  exclude?: string;
  tags?: string;
  language?: string;
  year_from?: number;
  year_to?: number;
  rating_min?: number;
  age?: AgeFilter;
  sort?: SortKey;
  order?: 'asc' | 'desc';
  view?: 'grid' | 'list';
}

export const KINDS: SeriesKind[] = [
  'manga',
  'manhwa',
  'manhua',
  'webtoon',
  'comic',
  'novel',
  'book',
];
export const STATUSES: SeriesStatus[] = ['ongoing', 'completed', 'hiatus', 'cancelled'];
export const AGES: AgeFilter[] = ['include', 'hide', 'only'];
export const SORT_KEYS: SortKey[] = ['updated', 'newest', 'title', 'followers', 'rating'];

const oneOf = <T extends string>(v: unknown, all: readonly T[]): T | undefined =>
  typeof v === 'string' && (all as readonly string[]).includes(v) ? (v as T) : undefined;
const str = (v: unknown) => (typeof v === 'string' && v.trim() ? v.trim() : undefined);
const num = (v: unknown) => {
  const n = typeof v === 'string' ? Number(v) : v;
  return typeof n === 'number' && Number.isInteger(n) ? n : undefined;
};

/** Router `validateSearch`: unknown keys and bad values are dropped, never thrown. */
export function validateBrowseSearch(s: Record<string, unknown>): BrowseSearch {
  const out: BrowseSearch = {};
  const set = <K extends keyof BrowseSearch>(k: K, v: BrowseSearch[K] | undefined) => {
    if (v !== undefined) out[k] = v;
  };
  set('q', str(s.q));
  set('kind', oneOf(s.kind, KINDS));
  set('status', oneOf(s.status, STATUSES));
  set('genres', str(s.genres));
  set('exclude', str(s.exclude));
  set('tags', str(s.tags));
  set('language', str(s.language));
  set('year_from', num(s.year_from));
  set('year_to', num(s.year_to));
  set(
    'rating_min',
    ((n) => (n !== undefined && n >= 1 && n <= 10 ? n : undefined))(num(s.rating_min)),
  );
  set('age', oneOf(s.age, AGES));
  set('sort', oneOf(s.sort, SORT_KEYS));
  set('order', oneOf(s.order, ['asc', 'desc'] as const));
  set('view', oneOf(s.view, ['grid', 'list'] as const));
  return out;
}
