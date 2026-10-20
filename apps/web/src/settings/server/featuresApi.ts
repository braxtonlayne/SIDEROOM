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

// ---- slowmode presets applied on demand (FTR-SLOW-001, G-74)

export interface SlowmodePresets {
  /** Whether the Slowmode schedules module is on in this server. */
  enabled: boolean;
  presets: { id: string; name: string; seconds: number }[];
}

export interface AppliedSlowmode {
  channel_id: string;
  /** Slowmode before and after, in seconds. */
  before: number;
  seconds: number;
  /** Unix milliseconds at which the old value comes back, when a duration was given. */
  revert_at: number | null;
}

export const getSlowmodePresets = (guildId: string) =>
  apiFetch<SlowmodePresets>(`/guilds/${guildId}/sr/slowmode/presets`, { retries: 0 });

export const applySlowmode = (
  guildId: string,
  body: { preset_id: string; channel_ids: string[]; minutes?: number },
) =>
  apiFetch<{ applied: AppliedSlowmode[] }>(`/guilds/${guildId}/sr/slowmode/apply`, {
    method: 'POST',
    retries: 0,
    json: body,
  });

// ---- channel and user scope (LNG-CHN-017, LNG-USET-036): crates/sr-api/src/routes/settings/features_scopes.rs

/** One module as a channel sees it. */
export interface ChannelFeature {
  id: string;
  name: string;
  description: string;
  section: string;
  /** Whether the module is on in the server; a channel cannot switch it on by itself. */
  guild_enabled: boolean;
  /** The channel's own switch, `null` when it follows the server. */
  override_enabled: boolean | null;
  /** On in this channel: the server's switch narrowed by the channel's. */
  enabled: boolean;
  platform_disabled: boolean;
  /** Effective settings in this channel. */
  settings: Record<string, unknown>;
  /** What the channel would use with no overrides of its own (defaults plus the server's settings). */
  inherited: Record<string, unknown>;
  /** Only what the channel overrides. */
  overrides: Record<string, unknown>;
  schema: unknown;
}

export const channelFeaturesPath = (channelId: string) => `/channels/${channelId}/sr/features`;

export const listChannelFeatures = (channelId: string) =>
  apiFetch<ChannelFeature[]>(channelFeaturesPath(channelId), { retries: 0 });

export const patchChannelFeature = (channelId: string, featureId: string, body: PatchFeatureBody) =>
  apiFetch<ChannelFeature>(`${channelFeaturesPath(channelId)}/${encodeURIComponent(featureId)}`, {
    method: 'PATCH',
    retries: 0,
    json: body,
  });

export const resetChannelFeature = (channelId: string, featureId: string) =>
  apiFetch<void>(`${channelFeaturesPath(channelId)}/${encodeURIComponent(featureId)}`, {
    method: 'DELETE',
    retries: 0,
  });

/** One module as the signed-in person sees it. */
export interface UserFeature {
  id: string;
  name: string;
  description: string;
  section: string;
  settings: Record<string, unknown>;
  overrides: Record<string, unknown>;
  schema: unknown;
}

export const listUserFeatures = () =>
  apiFetch<UserFeature[]>('/users/@me/sr/features', { retries: 0 });

export const patchUserFeature = (featureId: string, settings: Record<string, unknown>) =>
  apiFetch<UserFeature>(`/users/@me/sr/features/${encodeURIComponent(featureId)}`, {
    method: 'PATCH',
    retries: 0,
    json: { settings },
  });

export const resetUserFeature = (featureId: string) =>
  apiFetch<void>(`/users/@me/sr/features/${encodeURIComponent(featureId)}`, {
    method: 'DELETE',
    retries: 0,
  });
