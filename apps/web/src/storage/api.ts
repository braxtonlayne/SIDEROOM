import { apiFetch, apiPost } from '../api/client';

// Read side of BYOS-SCP-002 (ASSUMED shapes, see docs/dev/handoff/feat-p27-byos-ui.md).

export interface ObjectEntry {
  key: string;
  size: number;
  last_modified: string | null;
  content_type?: string | null;
}
/** `GET /storage/mounts/{id}/objects?prefix=&delimiter=/&cursor=`: one page of up to 1000 entries. */
export interface ObjectPage {
  /** Full prefixes of the sub-folders, each ending in `/`. */
  folders: string[];
  objects: ObjectEntry[];
  next_cursor: string | null;
}
/** `POST /storage/mounts/{id}/presign` answer. The URL is never stored; the client drops it at `expires_at`. */
export interface Presigned {
  url: string;
  expires_at: string;
}

export const listObjects = (mountId: string, prefix: string, cursor?: string | null) => {
  const q = new URLSearchParams({ prefix, delimiter: '/' });
  if (cursor) q.set('cursor', cursor);
  return apiFetch<ObjectPage>(`/storage/mounts/${encodeURIComponent(mountId)}/objects?${q}`, {
    retries: 0,
  });
};

export const presignGet = (mountId: string, key: string) =>
  apiPost<Presigned>(`/storage/mounts/${encodeURIComponent(mountId)}/presign`, {
    key,
    method: 'GET',
  });
