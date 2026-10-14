// URL search params of the browse pages. Kept apart from Browse.tsx so the router can validate them without
// pulling the page (and the UI kit) into the entry bundle.
import type { AgeFilter, SortKey } from './types';

export interface BrowseSearch {
  /** Search page only. */
  q?: string;
  genres?: string;
  collection?: string;
  year_from?: number;
  year_to?: number;
  /** Content rating, e.g. "PG-13". */
  rating?: string;
  runtime_max?: number;
  language?: string;
  subtitle_language?: string;
  age?: AgeFilter;
  sort?: SortKey;
  /** Opens the filters drawer (the search overlay's "Filters" shortcut). */
  filters?: 1;
}

const oneOf = <T extends string>(v: unknown, all: readonly T[]): T | undefined =>
  typeof v === 'string' && (all as readonly string[]).includes(v) ? (v as T) : undefined;
const str = (v: unknown) =>
  typeof v === 'string' || typeof v === 'number' ? String(v) : undefined;
const num = (v: unknown) => {
  const n = typeof v === 'number' ? v : typeof v === 'string' ? Number(v) : NaN;
  return Number.isFinite(n) ? n : undefined;
};

export function validateBrowseSearch(s: Record<string, unknown>): BrowseSearch {
  const o: BrowseSearch = {};
  const set = <K extends keyof BrowseSearch>(k: K, v: BrowseSearch[K]) => {
    if (v !== undefined && v !== '') o[k] = v;
  };
  set('q', str(s.q));
  set('genres', str(s.genres));
  set('collection', str(s.collection));
  set('year_from', num(s.year_from));
  set('year_to', num(s.year_to));
  set('rating', str(s.rating));
  set('runtime_max', num(s.runtime_max));
  set('language', str(s.language));
  set('subtitle_language', str(s.subtitle_language));
  set('age', oneOf(s.age, ['include', 'hide', 'only'] as const));
  set('sort', oneOf(s.sort, ['updated', 'newest', 'title', 'year', 'runtime'] as const));
  if (s.filters === 1 || s.filters === '1') o.filters = 1;
  return o;
}
