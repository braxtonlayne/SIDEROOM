import { apiFetch, apiPost } from '../api/client';
import type { GuestbookEntry, ProfileDoc, ProfileMedia, ProfilePage } from './types';

const id = encodeURIComponent;
const ro = { retries: 0 } as const;

export const getPageByName = (username: string) =>
  apiFetch<ProfilePage>(`/profile-pages/${id(username)}`, ro);
export const getPageById = (userId: string) =>
  apiFetch<ProfilePage>(`/users/${userId === '@me' ? '@me' : id(userId)}/profile-page`, ro);
export const getOwnDoc = () => apiFetch<ProfileDoc>('/users/@me/profile-doc', ro);
export const saveDoc = (doc: ProfileDoc) =>
  apiFetch<ProfileDoc>('/users/@me/profile-doc', { method: 'PUT', json: doc, ...ro });
export const adoptLayout = (packageId: string) =>
  apiPost<ProfileDoc>('/users/@me/profile-doc/layout', { package_id: packageId });

export const listMedia = () => apiFetch<ProfileMedia[]>('/users/@me/profile-media', ro);
/** Upload a background or gallery picture (the file is the request body). */
export const uploadMedia = async (kind: 'background' | 'gallery', file: File) =>
  apiFetch<ProfileMedia>(`/users/@me/profile-media?kind=${kind}`, {
    method: 'POST',
    // The bytes and the type the file carries; the server sniffs the real type and ignores this one.
    raw: await readBytes(file),
    headers: { 'content-type': file.type || 'application/octet-stream' },
    ...ro,
  });

function readBytes(file: Blob): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === 'function') return file.arrayBuffer();
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as ArrayBuffer);
    r.onerror = () => reject(r.error);
    r.readAsArrayBuffer(file);
  });
}
export const deleteMedia = (mediaId: string) =>
  apiFetch<void>(`/users/@me/profile-media/${id(mediaId)}`, { method: 'DELETE', ...ro });

export const listGuestbook = (userId: string, before?: string) =>
  apiFetch<GuestbookEntry[]>(
    `/users/${id(userId)}/guestbook${before ? `?before=${id(before)}` : ''}`,
    ro,
  );
export const signGuestbook = (userId: string, body: string) =>
  apiPost<GuestbookEntry>(`/users/${id(userId)}/guestbook`, { body });
export const approveGuestbookEntry = (entryId: string) =>
  apiFetch<void>(`/users/@me/guestbook/${id(entryId)}/approve`, { method: 'POST', ...ro });
export const removeGuestbookEntry = (userId: string, entryId: string) =>
  apiFetch<void>(`/users/${id(userId)}/guestbook/${id(entryId)}`, { method: 'DELETE', ...ro });
