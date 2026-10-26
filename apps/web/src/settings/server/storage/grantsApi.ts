import { apiFetch } from '../../../api/client';

/** The five storage grants (roadmap-p27 section 3). Rows of `guild_storage_grants`, not permission bits. */
export const STORAGE_GRANTS = [
  'view_cinema',
  'view_library',
  'view_music',
  'upload',
  'manage',
] as const;
export type StorageGrant = (typeof STORAGE_GRANTS)[number];

/**
 * ASSUMED shape of BYOS-SCP-001 `PUT /guilds/{guild_id}/storage/grants` (MANAGE_ROLES plus `manage`):
 * body `{ role_id, grants }` replaces the role's whole grant set; answer 204.
 * The role payload carries the current set as `sr_storage_grants`.
 */
export const putStorageGrants = (guildId: string, roleId: string, grants: StorageGrant[]) =>
  apiFetch<void>(`/guilds/${encodeURIComponent(guildId)}/storage/grants`, {
    method: 'PUT',
    json: { role_id: roleId, grants },
    retries: 0,
  });
