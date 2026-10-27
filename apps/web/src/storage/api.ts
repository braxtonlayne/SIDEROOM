import { apiFetch, apiPost } from '../api/client';

// Read side of BYOS-SCP-002; shapes checked against the merged OpenAPI (ObjectList, ObjectEntry, PresignView).

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
  /** The caller may upload and make folders here. Absent on an older server: read-only. */
  can_upload?: boolean;
  /** The caller may rename, move and delete here. */
  can_manage?: boolean;
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

// ---- Write side (BYOS-BRW-001); shapes from crates/sr-api/src/routes/storage/{uploads,objects}.rs ----

const mountPath = (mountId: string) => `/storage/mounts/${encodeURIComponent(mountId)}`;
const uploadPath = (mountId: string, uploadId: string) =>
  `${mountPath(mountId)}/uploads/${encodeURIComponent(uploadId)}`;
/** Encode a key for a path: each segment, slashes kept (a trailing slash stays: it names a folder). */
export const keyPath = (key: string) => key.split('/').map(encodeURIComponent).join('/');

/** One upload in the server's ledger. The bucket's own upload id never reaches the browser. */
export interface UploadSlot {
  id: string;
  key: string;
  size: number;
  /** Bytes per part (16 MiB); the last part is smaller. */
  part_size: number;
  part_count: number;
  content_type: string;
  created_at: string;
}
export interface UploadPartInfo {
  number: number;
  size: number;
  etag: string;
}
export interface UploadStatus {
  upload: UploadSlot;
  parts: UploadPartInfo[];
}
/** A presigned part `PUT`; valid for an hour, so it is asked for just before the part goes out. */
export interface PartUrl {
  part_number: number;
  size: number;
  url: string;
  expires_at: string;
}

export const beginUpload = (
  mountId: string,
  body: { key: string; size: number; content_type?: string },
) => apiPost<UploadSlot>(`${mountPath(mountId)}/uploads`, body);
export const listUploads = (mountId: string) =>
  apiFetch<UploadSlot[]>(`${mountPath(mountId)}/uploads`, { retries: 0 });
export const uploadStatus = (mountId: string, uploadId: string) =>
  apiFetch<UploadStatus>(uploadPath(mountId, uploadId), { retries: 0 });
export const partUrl = (mountId: string, uploadId: string, part: number) =>
  apiPost<PartUrl>(`${uploadPath(mountId, uploadId)}/parts/${part}`);
export const completeUpload = (mountId: string, uploadId: string) =>
  apiPost<{ key: string; size: number }>(`${uploadPath(mountId, uploadId)}/complete`, {});
export const abortUpload = (mountId: string, uploadId: string) =>
  apiFetch<void>(uploadPath(mountId, uploadId), { method: 'DELETE', retries: 0 });

/** `key` ends in a slash. */
export const createFolder = (mountId: string, key: string) =>
  apiPost<{ key: string }>(`${mountPath(mountId)}/folders`, { key });
/** Rename and move are one call: an object to an object, a folder (both ending in a slash) to a folder. */
export const moveObjects = (mountId: string, from: string, to: string) =>
  apiPost<{ count: number }>(`${mountPath(mountId)}/move`, { from, to });
/** A key ending in a slash deletes the folder and everything in it. */
export const deleteObjects = (mountId: string, key: string) =>
  apiFetch<{ count: number }>(`${mountPath(mountId)}/objects/${keyPath(key)}`, {
    method: 'DELETE',
    retries: 0,
  });
