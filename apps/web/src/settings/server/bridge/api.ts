// REST helpers for the Discord bridge (BRG-UI-001). Paths and shapes follow docs/dev/roadmap-p27.md section 13 and
// crates/sr-api/src/routes/guilds/bridge.rs (bridge-core); everything that is a guess about the wire is in this one
// file so it can be reconciled with the server in one place.
//
// Owner-only calls (module switch, token) pass through `withReauth` because the server demands the MFA step for them
// (D-381); the mapping, test and status calls are MANAGE_GUILD and answer 404 while the module is off.
import { apiFetch } from '../../../api/client';
import { withReauth } from '../../../admin/reauth';
import type { GuildFeature } from '../featuresApi';

/** Module id of the guild switch (crates/sr-features/src/modules/bridge.rs). */
export const BRIDGE_MODULE_ID = 'bridge.enabled';

export type BridgeDirection = 'both' | 'to_discord' | 'from_discord';

export type BridgeState = 'no_token' | 'connecting' | 'connected' | 'error';

export interface BridgeMap {
  /** SIDE ROOM channel. */
  channel_id: string;
  discord_channel_id: string;
  /** The channel's name as the bot sees it, for display. */
  discord_channel_name?: string | null;
  direction: BridgeDirection;
}

export interface BridgeStatus {
  state: BridgeState;
  /** A sealed bot token is stored (the token itself never comes back). */
  token_set: boolean;
  discord_guild_id?: string | null;
  discord_guild_name?: string | null;
  /** Whether Discord granted the privileged MESSAGE_CONTENT intent (the owner enables it in the developer portal). */
  message_content_intent?: boolean;
  /** Invite link for the bot with the permissions it needs; present once a token is set. */
  invite_url?: string | null;
  /** Last problem the shard reported, as plain text. */
  last_error?: string | null;
  maps: BridgeMap[];
}

export interface DiscordChannel {
  id: string;
  name: string;
}

/** What the switch-off left behind: the PATCH response carries it next to the module. */
export interface TeardownReport {
  maps_removed: number;
  /** Webhooks Discord would not delete after three attempts; the owner removes them by hand. */
  failed_webhooks: string[];
}

const path = (guildId: string, rest = '') => `/guilds/${guildId}/bridge${rest}`;

const send = <T>(p: string, method: string, json?: unknown) =>
  withReauth(() => apiFetch<T>(p, { method, retries: 0, ...(json !== undefined ? { json } : {}) }));

export const getBridge = (guildId: string) => apiFetch<BridgeStatus>(path(guildId), { retries: 0 });

export const putToken = (guildId: string, token: string) =>
  send<BridgeStatus>(path(guildId, '/token'), 'PUT', { token });

export const deleteToken = (guildId: string) => send<void>(path(guildId, '/token'), 'DELETE');

export const listDiscordChannels = (guildId: string) =>
  apiFetch<DiscordChannel[]>(path(guildId, '/discord-channels'), { retries: 0 });

export const createMap = (
  guildId: string,
  body: { channel_id: string; discord_channel_id: string; direction: BridgeDirection },
) => send<BridgeMap>(path(guildId, '/maps'), 'POST', body);

export const deleteMap = (guildId: string, channelId: string) =>
  send<void>(path(guildId, `/maps/${channelId}`), 'DELETE');

export const testMap = (guildId: string, channelId: string) =>
  send<{ ok: boolean }>(path(guildId, '/test'), 'POST', { channel_id: channelId });

/** The Discord developer-portal page where the owner finds and enables the bot's privileged intent. */
export const DEVELOPER_PORTAL_URL = 'https://discord.com/developers/applications';
/** Where the owner removes a webhook by hand when the bridge could not (Discord > Server Settings > Integrations). */
export const INTEGRATIONS_HINT = 'Server Settings > Integrations > Webhooks';

// ---- the module switch (owner only; the one feature-toggle handler reads `owner_only`)

/** A module as the features list returns it, plus what the bridge adds. */
export type BridgeFeature = GuildFeature & {
  /** Declared by the module: only the guild owner switches it (with the MFA step). */
  owner_only?: boolean;
  /** Present on the answer to a switch-off. */
  teardown?: TeardownReport;
};

export const switchBridge = (guildId: string, enabled: boolean) =>
  send<BridgeFeature>(
    `/guilds/${guildId}/sr/features/${encodeURIComponent(BRIDGE_MODULE_ID)}`,
    'PATCH',
    {
      enabled,
    },
  );
