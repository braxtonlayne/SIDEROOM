// Typed fetchers for the personal Cinema pages. Routes are assumptions until docs/specs/cinema-media.md lists
// them (see docs/dev/handoff/feat-p6-cinema-player.md):
//   GET    /cinema/me/list                 -> {items: TitleCard[]}
//   DELETE /cinema/me/list/{titleId}
//   GET    /cinema/me/continue?limit=      -> {items: ContinueItem[]}
//   GET    /cinema/me/lists                -> {items: CinemaList[]}
//   GET    /cinema/me/lists/{id}           -> {list: CinemaList, items: TitleCard[]}
//   GET    /cinema/me/history?limit=       -> HistoryPage
//   PUT    /cinema/me/history              {paused: boolean}
//   DELETE /cinema/me/history
import { apiFetch } from '../../api/client';
import type { CinemaList, ContinueItem, HistoryPage, TitleCard } from './types';

const enc = encodeURIComponent;

export const getSaved = () => apiFetch<{ items: TitleCard[] }>('/cinema/me/list');
export const removeSaved = (titleId: string) =>
  apiFetch<void>(`/cinema/me/list/${enc(titleId)}`, { method: 'DELETE', retries: 0 });
export const getContinue = (limit = 24) =>
  apiFetch<{ items: ContinueItem[] }>(`/cinema/me/continue?limit=${limit}`);
export const getLists = () => apiFetch<{ items: CinemaList[] }>('/cinema/me/lists');
export const getList = (id: string) =>
  apiFetch<{ list: CinemaList; items: TitleCard[] }>(`/cinema/me/lists/${enc(id)}`);
export const getHistory = (limit = 50) =>
  apiFetch<HistoryPage>(`/cinema/me/history?limit=${limit}`);
export const setHistoryPaused = (paused: boolean) =>
  apiFetch<void>('/cinema/me/history', { method: 'PUT', json: { paused }, retries: 0 });
export const clearHistory = () =>
  apiFetch<void>('/cinema/me/history', { method: 'DELETE', retries: 0 });

export const keys = {
  saved: ['cinema', 'my', 'saved'] as const,
  continue: ['cinema', 'my', 'continue'] as const,
  lists: ['cinema', 'my', 'lists'] as const,
  list: (id: string) => ['cinema', 'my', 'list', id] as const,
  history: ['cinema', 'my', 'history'] as const,
};
