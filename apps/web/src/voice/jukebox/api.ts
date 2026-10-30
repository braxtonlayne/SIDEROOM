// Typed fetchers for the jukebox routes. The controller and the components never build URLs.
import { apiFetch } from '../../api/client';
import type { JukeboxApi, JukeboxState, LibraryPage } from './types';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;
const base = (channelId: string) => `/channels/${enc(channelId)}/jukebox`;

export const httpJukeboxApi: JukeboxApi = {
  state: (c) => apiFetch<JukeboxState>(base(c), NO_RETRY),
  setMode: (c, mode) =>
    apiFetch<JukeboxState>(base(c), { method: 'PUT', json: { mode }, ...NO_RETRY }),
  enqueue: (c, track_ids) =>
    apiFetch<JukeboxState>(`${base(c)}/queue`, {
      method: 'POST',
      json: { track_ids },
      ...NO_RETRY,
    }),
  remove: (c, item) =>
    apiFetch<JukeboxState>(`${base(c)}/queue/${enc(item)}`, { method: 'DELETE', ...NO_RETRY }),
  voteSkip: (c) => apiFetch<JukeboxState>(`${base(c)}/skip`, { method: 'POST', ...NO_RETRY }),
  withdrawVote: (c) => apiFetch<JukeboxState>(`${base(c)}/skip`, { method: 'DELETE', ...NO_RETRY }),
  skipNow: (c) => apiFetch<JukeboxState>(`${base(c)}/next`, { method: 'POST', ...NO_RETRY }),
  ended: (c, item_id) =>
    apiFetch<JukeboxState>(`${base(c)}/ended`, { method: 'POST', json: { item_id }, ...NO_RETRY }),
  setDj: (c, user_id) =>
    apiFetch<JukeboxState>(`${base(c)}/dj`, { method: 'PUT', json: { user_id }, ...NO_RETRY }),
  setPaused: (c, paused) =>
    apiFetch<JukeboxState>(`${base(c)}/playback`, { method: 'PUT', json: { paused }, ...NO_RETRY }),
  listen: (c) => apiFetch<JukeboxState>(`${base(c)}/listening`, { method: 'PUT', ...NO_RETRY }),
  leave: (c) => apiFetch<JukeboxState>(`${base(c)}/listening`, { method: 'DELETE', ...NO_RETRY }),
  presign: (c, item_id) =>
    apiFetch<{ url: string }>(`${base(c)}/presign`, {
      method: 'POST',
      json: { item_id },
      ...NO_RETRY,
    }),
  library(q) {
    const params = new URLSearchParams({ limit: '30' });
    if (q.trim()) params.set('q', q.trim());
    return apiFetch<LibraryPage>(`/music/library?${params}`, NO_RETRY);
  },
};
