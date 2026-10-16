import { apiFetch, apiPost } from '../api/client';
import type { GuestbookEntry, ProfileDoc, ProfilePage } from './types';

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

export const listGuestbook = (userId: string, before?: string) =>
  apiFetch<GuestbookEntry[]>(
    `/users/${id(userId)}/guestbook${before ? `?before=${id(before)}` : ''}`,
    ro,
  );
export const signGuestbook = (userId: string, body: string) =>
  apiPost<GuestbookEntry>(`/users/${id(userId)}/guestbook`, { body });
export const removeGuestbookEntry = (userId: string, entryId: string) =>
  apiFetch<void>(`/users/${id(userId)}/guestbook/${id(entryId)}`, { method: 'DELETE', ...ro });
