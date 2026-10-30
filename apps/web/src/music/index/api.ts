// Server calls of the lazy tagger (MUS-IDX-001). Shapes: crates/sr-api/src/routes/music/{library,tracks}.rs.
import { apiFetch, apiPost } from '../../api/client';
import { getMountUploads } from '../../storage/browser/mountUploads';
import type { TaggerApi, TrackPage } from './types';

export const realTaggerApi: TaggerApi = {
  listPending(mountId, limit, offset) {
    const q = new URLSearchParams({
      pending: 'true',
      limit: String(limit),
      offset: String(offset),
    });
    if (mountId) q.set('mount', mountId);
    return apiFetch<TrackPage>(`/music/library?${q}`, { retries: 0 });
  },
  presign: (trackId) =>
    apiPost<{ url: string }>(`/music/tracks/${encodeURIComponent(trackId)}/presign`, {}),
  async patch(trackId, body) {
    await apiFetch<unknown>(`/music/tracks/${encodeURIComponent(trackId)}`, {
      method: 'PATCH',
      json: body,
      retries: 0,
    });
  },
  rescan: (mountId) => apiPost<{ queued: number }>('/music/library/rescan', { mount_id: mountId }),
  uploadCover: (mountId, key, blob) => getMountUploads().enqueue(mountId, key, blob),
};
