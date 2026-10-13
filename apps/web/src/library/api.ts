// Typed fetchers for the member routes under /library. All data access goes through here so components never
// build URLs. Progress and bookmarks live in progress.ts (their endpoints are added separately).
import { apiFetch } from '../api/client';
import type {
  BrowseParams,
  ChapterList,
  Follow,
  FollowPage,
  InstantSearch,
  PageManifest,
  SeriesDetail,
  SeriesPage,
  Shelf,
  UpdatesPage,
} from './types';

type Query = Record<string, string | number | boolean | string[] | undefined | null>;

export function qs(q: Query): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) {
    if (v === undefined || v === null || v === '') continue;
    if (Array.isArray(v)) {
      if (v.length) p.set(k, v.join(','));
    } else p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : '';
}

export const browseSeries = (params: BrowseParams = {}) =>
  apiFetch<SeriesPage>(`/library/series${qs({ ...params })}`);

export const getSeries = (id: string) =>
  apiFetch<SeriesDetail>(`/library/series/${encodeURIComponent(id)}`);

export const getChapters = (seriesId: string, edition?: string) =>
  apiFetch<ChapterList>(
    `/library/series/${encodeURIComponent(seriesId)}/chapters${qs({ edition })}`,
  );

export const getPages = (chapterId: string) =>
  apiFetch<PageManifest>(`/library/chapters/${encodeURIComponent(chapterId)}/pages`, {
    retries: 0,
  });

export const searchLibrary = (q: string, limit = 8, signal?: AbortSignal) =>
  apiFetch<InstantSearch>(`/library/search${qs({ q, limit })}`, { signal });

export const getUpdates = (opts: { following?: boolean; before?: string; limit?: number } = {}) =>
  apiFetch<UpdatesPage>(`/library/updates${qs({ ...opts })}`);

export const getFollows = (opts: { shelf?: Shelf; limit?: number; offset?: number } = {}) =>
  apiFetch<FollowPage>(`/library/me/follows${qs({ ...opts })}`);

export const followSeries = (id: string, body: { notify?: boolean; shelf?: Shelf } = {}) =>
  apiFetch<Follow>(`/library/series/${encodeURIComponent(id)}/follow`, {
    method: 'PUT',
    json: body,
    retries: 0,
  });

export const unfollowSeries = (id: string) =>
  apiFetch<void>(`/library/series/${encodeURIComponent(id)}/follow`, {
    method: 'DELETE',
    retries: 0,
  });

/** Query keys, so invalidation stays in one place. */
export const keys = {
  browse: (p: BrowseParams) => ['library', 'browse', p] as const,
  series: (id: string) => ['library', 'series', id] as const,
  chapters: (id: string, edition?: string) => ['library', 'chapters', id, edition ?? null] as const,
  updates: (following: boolean) => ['library', 'updates', following] as const,
  follows: (shelf?: Shelf) => ['library', 'follows', shelf ?? null] as const,
  search: (q: string) => ['library', 'search', q] as const,
};
