// Per-object URLs of a shelf (STU-ZK-001 / STU-SHF-001, wave 41): the Study reads every file body through
// `GET /shelves/{shelf_id}/objects/{object_hash}/url`, which checks the grant and every delisting at the moment it mints
// a presigned GET for that one object (ten minutes at most). The client never holds a bucket credential.
import { ApiError, apiFetch } from '../api/client';
import { presignGet, type Presigned } from '../storage/api';

const hex = (b: ArrayBuffer) =>
  [...new Uint8Array(b)].map((x) => x.toString(16).padStart(2, '0')).join('');

/** The catalog's `object_hash` of a key: lowercase hex SHA-256 of the mount-relative path (UTF-8). */
export async function objectHash(key: string): Promise<string> {
  return hex(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(key)));
}

export const objectPath = (shelfId: string, hash: string) =>
  `/shelves/${encodeURIComponent(shelfId)}/objects/${hash}`;

/**
 * A short-lived URL for one file of a shelf. A file the indexer has not reached yet is not in the catalog (404); only
 * then the mount's own per-key presign is asked, which applies the same scope and delisting rules.
 */
export async function shelfObjectUrl(shelfId: string, key: string): Promise<Presigned> {
  const hash = await objectHash(key);
  try {
    return await apiFetch<Presigned>(`${objectPath(shelfId, hash)}/url`, { retries: 0 });
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) return presignGet(shelfId, key);
    throw e;
  }
}
