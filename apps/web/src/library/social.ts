// Typed fetchers for the social routes under /library: ratings, reviews, reading lists, rankings and
// unread counts (crates/sr-api/src/routes/library/social.rs; spec library-reader sections 4 and 6).
import { apiFetch } from '../api/client';
import { qs } from './api';
import type { Series } from './types';

export interface RatingAggregate {
  average?: number | null;
  count: number;
}

export type RatingScope = 'global' | 'known' | 'friends';

export interface SeriesRatings {
  mine?: number | null;
  global: RatingAggregate;
  friends: RatingAggregate;
  known: RatingAggregate;
  /** Counts of scores 1 to 10 (index 0 is score 1). */
  distribution: number[];
}

export interface ChapterRating {
  mine?: number | null;
  average?: number | null;
  count: number;
}

export interface Review {
  id: string;
  series_id: string;
  author_id: string;
  author_name: string;
  rating?: number | null;
  body: string;
  spoiler: boolean;
  helpful_count: number;
  voted: boolean;
  mine: boolean;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface ReviewPage {
  items: Review[];
  next_before?: string | null;
}

export type ReviewSort = 'helpful' | 'recent';
export type ListVisibility = 'private' | 'friends' | 'public';

export interface ReadingList {
  id: string;
  owner_id: string;
  owner_name: string;
  mine: boolean;
  name: string;
  description: string;
  visibility: ListVisibility;
  featured: boolean;
  item_count: number;
  contains?: boolean | null;
  covers: string[];
  created_at_ms: number;
  updated_at_ms: number;
}

export interface ListItem {
  series: Series;
  position: number;
  note: string;
  added_at_ms: number;
}

export interface ListDetail {
  list: ReadingList;
  items: ListItem[];
}

export type RankingPeriod = 'week' | 'month' | 'all';

export interface RankingEntry {
  rank: number;
  score: number;
  readers: number;
  rating_average?: number | null;
  rating_count: number;
  series: Series;
}

export interface UnreadCount {
  series_id: string;
  unread: number;
  last_read_at_ms?: number | null;
  chapter_count: number;
}

const enc = encodeURIComponent;

/* ---------------------------------------------------------------- ratings */

export const getRatings = (seriesId: string) =>
  apiFetch<SeriesRatings>(`/library/series/${enc(seriesId)}/ratings`);

export const rateSeries = (seriesId: string, score: number) =>
  apiFetch<void>(`/library/series/${enc(seriesId)}/rating`, {
    method: 'PUT',
    json: { score },
    retries: 0,
  });

export const unrateSeries = (seriesId: string) =>
  apiFetch<void>(`/library/series/${enc(seriesId)}/rating`, { method: 'DELETE', retries: 0 });

export const getChapterRating = (chapterId: string) =>
  apiFetch<ChapterRating>(`/library/chapters/${enc(chapterId)}/rating`, { retries: 0 });

export const rateChapter = (chapterId: string, score: number) =>
  apiFetch<void>(`/library/chapters/${enc(chapterId)}/rating`, {
    method: 'PUT',
    json: { score },
    retries: 0,
  });

export const unrateChapter = (chapterId: string) =>
  apiFetch<void>(`/library/chapters/${enc(chapterId)}/rating`, { method: 'DELETE', retries: 0 });

/* ---------------------------------------------------------------- reviews */

export const getReviews = (
  seriesId: string,
  o: { sort?: ReviewSort; before?: string; limit?: number } = {},
) => apiFetch<ReviewPage>(`/library/series/${enc(seriesId)}/reviews${qs({ ...o })}`);

export const putReview = (
  seriesId: string,
  body: { body: string; rating?: number | null; spoiler?: boolean },
) =>
  apiFetch<Review>(`/library/series/${enc(seriesId)}/review`, {
    method: 'PUT',
    json: { ...body, rating: body.rating ?? undefined },
    retries: 0,
  });

export const deleteReview = (seriesId: string) =>
  apiFetch<void>(`/library/series/${enc(seriesId)}/review`, { method: 'DELETE', retries: 0 });

export const setHelpful = (reviewId: string, on: boolean) =>
  apiFetch<{ count: number }>(`/library/reviews/${enc(reviewId)}/helpful`, {
    method: on ? 'PUT' : 'DELETE',
    retries: 0,
  });

/* ------------------------------------------------------------------ lists */

export const getMyLists = (series?: string) =>
  apiFetch<{ items: ReadingList[] }>(`/library/me/lists${qs({ series })}`);

export const getFeaturedLists = (limit = 12) =>
  apiFetch<{ items: ReadingList[] }>(`/library/lists/featured${qs({ limit })}`);

export const getList = (id: string) => apiFetch<ListDetail>(`/library/lists/${enc(id)}`);

export const createList = (body: {
  name: string;
  description?: string;
  visibility?: ListVisibility;
}) => apiFetch<ReadingList>('/library/lists', { method: 'POST', json: body, retries: 0 });

export const updateList = (
  id: string,
  body: { name?: string; description?: string; visibility?: ListVisibility },
) =>
  apiFetch<ReadingList>(`/library/lists/${enc(id)}`, {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

export const deleteList = (id: string) =>
  apiFetch<void>(`/library/lists/${enc(id)}`, { method: 'DELETE', retries: 0 });

export const addToList = (listId: string, seriesId: string, note?: string) =>
  apiFetch<ReadingList>(`/library/lists/${enc(listId)}/items/${enc(seriesId)}`, {
    method: 'PUT',
    json: note ? { note } : {},
    retries: 0,
  });

export const removeFromList = (listId: string, seriesId: string) =>
  apiFetch<ReadingList>(`/library/lists/${enc(listId)}/items/${enc(seriesId)}`, {
    method: 'DELETE',
    retries: 0,
  });

export const reorderList = (listId: string, seriesIds: string[]) =>
  apiFetch<ReadingList>(`/library/lists/${enc(listId)}/order`, {
    method: 'PUT',
    json: { series_ids: seriesIds },
    retries: 0,
  });

/* ------------------------------------------------------ rankings, unread */

/** `friends` and `known` count only members who share their reading (D-190). */
export type RankingScope = 'global' | 'friends' | 'known';

export const getRankings = (
  o: { period?: RankingPeriod; scope?: RankingScope; kind?: string; limit?: number } = {},
) =>
  apiFetch<{ period: RankingPeriod; scope: RankingScope; items: RankingEntry[] }>(
    `/library/rankings${qs({ ...o })}`,
  );

export const getUnread = () => apiFetch<{ items: UnreadCount[] }>('/library/me/unread');

/** Query keys for invalidation. */
export const socialKeys = {
  ratings: (seriesId: string) => ['library', 'ratings', seriesId] as const,
  reviews: (seriesId: string, sort: ReviewSort) => ['library', 'reviews', seriesId, sort] as const,
  myLists: (series?: string) => ['library', 'lists', 'mine', series ?? null] as const,
  featured: () => ['library', 'lists', 'featured'] as const,
  list: (id: string) => ['library', 'lists', 'one', id] as const,
  rankings: (period: RankingPeriod, kind?: string, scope: RankingScope = 'global') =>
    ['library', 'rankings', period, kind ?? null, scope] as const,
  unread: () => ['library', 'unread'] as const,
};
