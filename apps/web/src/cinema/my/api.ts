// Typed fetchers for the personal Cinema pages, against the social contract (docs/specs/cinema-media.md 8):
//   GET    /cinema/me/list?limit=             -> {items, next_before}   (saved titles, newest first)
//   DELETE /cinema/me/list/{title_id}
//   GET    /cinema/me/continue-watching       -> {items: ContinueItem[]}
//   GET    /cinema/lists                      -> {lists: CinemaList[]}  (the saved one first)
//   GET    /cinema/lists/{id}                 -> {list, items: ListEntry[], next_before}
//   GET    /cinema/me/history?limit=          -> {items: HistoryItem[], next_before}
//   GET|PUT /cinema/me/history/settings       {paused}
//   DELETE /cinema/me/history
import { apiFetch } from '../../api/client';
import type { CinemaList, ContinueItem, HistoryItem, ListEntry, TitleRef } from './types';

const enc = encodeURIComponent;

/** The saved list answers titles; read both `{title}` and bare title items so either spelling works. */
export async function getSaved(limit = 100): Promise<TitleRef[]> {
  const r = await apiFetch<{ items: (TitleRef | { title: TitleRef })[] }>(
    `/cinema/me/list?limit=${limit}`,
  );
  // A bare TitleRef has a string `title`; a wrapper `{title, ...}` has an object.
  return r.items.map((it) => (typeof it.title === 'object' ? it.title : (it as TitleRef)));
}
export const removeSaved = (titleId: string) =>
  apiFetch<void>(`/cinema/me/list/${enc(titleId)}`, { method: 'DELETE', retries: 0 });

export const getContinue = (limit = 24) =>
  apiFetch<{ items: ContinueItem[] }>(`/cinema/me/continue-watching?limit=${limit}`);

export async function getLists(): Promise<CinemaList[]> {
  const r = await apiFetch<{ lists: CinemaList[] }>('/cinema/lists');
  return r.lists.filter((l) => l.kind === 'custom');
}
export const getList = (id: string) =>
  apiFetch<{ list: CinemaList; items: ListEntry[] }>(`/cinema/lists/${enc(id)}`);

export const getHistory = (limit = 100) =>
  apiFetch<{ items: HistoryItem[] }>(`/cinema/me/history?limit=${limit}`);
export const getHistorySettings = () =>
  apiFetch<{ paused: boolean }>('/cinema/me/history/settings');
export const setHistoryPaused = (paused: boolean) =>
  apiFetch<void>('/cinema/me/history/settings', { method: 'PUT', json: { paused }, retries: 0 });
export const clearHistory = () =>
  apiFetch<void>('/cinema/me/history', { method: 'DELETE', retries: 0 });

export const keys = {
  saved: ['cinema', 'my', 'saved'] as const,
  continue: ['cinema', 'my', 'continue'] as const,
  lists: ['cinema', 'my', 'lists'] as const,
  list: (id: string) => ['cinema', 'my', 'list', id] as const,
  history: ['cinema', 'my', 'history'] as const,
  historySettings: ['cinema', 'my', 'history-settings'] as const,
};
