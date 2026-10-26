import { apiFetch, apiPost } from '../../../api/client';
import type { CorsResult, Mount, MountCreate, MountPatch, Share, TestResult } from './types';

/** Where a set of mounts lives: the caller's own, or a guild's (needs the `manage` grant). */
export type MountScope = { kind: 'user' } | { kind: 'guild'; guildId: string };

const base = (s: MountScope) =>
  s.kind === 'user'
    ? '/users/@me/storage/mounts'
    : `/guilds/${encodeURIComponent(s.guildId)}/storage/mounts`;
const one = (s: MountScope, id: string) => `${base(s)}/${encodeURIComponent(id)}`;

export const listMounts = (s: MountScope) => apiFetch<Mount[]>(base(s), { retries: 0 });
export const createMount = (s: MountScope, body: MountCreate) => apiPost<Mount>(base(s), body);
export const patchMount = (s: MountScope, id: string, body: MountPatch) =>
  apiFetch<Mount>(one(s, id), { method: 'PATCH', json: body, retries: 0 });
export const deleteMount = (s: MountScope, id: string) =>
  apiFetch<void>(one(s, id), { method: 'DELETE', retries: 0 });
export const testMount = (s: MountScope, id: string) => apiPost<TestResult>(`${one(s, id)}/test`);
export const checkCors = (s: MountScope, id: string) =>
  apiPost<CorsResult>(`${one(s, id)}/cors/check`);
export const applyCors = (s: MountScope, id: string) =>
  apiPost<CorsResult>(`${one(s, id)}/cors/apply`);

// ---- Friend Shelf shares ----
// PUT/DELETE /users/@me/storage/mounts/{mount_id}/shares/{user_id} are in the roadmap (PUT body = the
// three flags, answer the stored Share; DELETE revokes). GET .../mounts/{mount_id}/shares (the owner's
// outgoing list) is ASSUMED: the roadmap names only GET /users/@me/storage/shares (incoming).
const shareBase = (mountId: string) =>
  `/users/@me/storage/mounts/${encodeURIComponent(mountId)}/shares`;
export const listShares = (mountId: string) =>
  apiFetch<Share[]>(shareBase(mountId), { retries: 0 });
export const putShare = (mountId: string, friendId: string, flags: Omit<Share, 'friend_id'>) =>
  apiFetch<Share>(`${shareBase(mountId)}/${encodeURIComponent(friendId)}`, {
    method: 'PUT',
    json: flags,
    retries: 0,
  });
export const deleteShare = (mountId: string, friendId: string) =>
  apiFetch<void>(`${shareBase(mountId)}/${encodeURIComponent(friendId)}`, {
    method: 'DELETE',
    retries: 0,
  });
