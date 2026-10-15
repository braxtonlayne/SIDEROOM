// REST helpers for the server's feature modules and temporary roles (FTR-*). Paths and shapes mirror
// crates/sr-api/src/routes/settings/features.rs.
import { apiFetch } from '../../api/client';

export interface GuildFeature {
  id: string;
  name: string;
  description: string;
  section: string;
  phase: number;
  /** Whether the module is on in this server. */
  enabled: boolean;
  default_enabled: boolean;
  /** The platform operator switched it off everywhere. */
  platform_disabled: boolean;
  /** Effective settings: schema defaults plus this server's overrides. */
  settings: Record<string, unknown>;
  /** Settings schema (crates/sr-features/src/schema.rs). */
  schema: unknown;
}

export interface PatchFeatureBody {
  enabled?: boolean;
  settings?: Record<string, unknown>;
}

export interface TempRole {
  user_id: string;
  role_id: string;
  /** Unix milliseconds. */
  expires_at: number;
}

export const FEATURES_PATH = (guildId: string) => `/guilds/${guildId}/sr/features`;

export const listFeatures = (guildId: string) =>
  apiFetch<GuildFeature[]>(FEATURES_PATH(guildId), { retries: 0 });

export const patchFeature = (guildId: string, featureId: string, body: PatchFeatureBody) =>
  apiFetch<GuildFeature>(`${FEATURES_PATH(guildId)}/${encodeURIComponent(featureId)}`, {
    method: 'PATCH',
    retries: 0,
    json: body,
  });

export const listTempRoles = (guildId: string) =>
  apiFetch<TempRole[]>(`/guilds/${guildId}/sr/temp-roles`, { retries: 0 });

export const grantTempRole = (
  guildId: string,
  body: { user_id: string; role_id: string; duration_seconds: number },
) =>
  apiFetch<TempRole>(`/guilds/${guildId}/sr/temp-roles`, {
    method: 'POST',
    retries: 0,
    json: body,
  });

export const revokeTempRole = (guildId: string, userId: string, roleId: string) =>
  apiFetch<void>(`/guilds/${guildId}/sr/temp-roles/${userId}/${roleId}`, {
    method: 'DELETE',
    retries: 0,
  });
