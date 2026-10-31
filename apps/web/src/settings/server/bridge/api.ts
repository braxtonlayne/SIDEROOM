// REST helpers for the Discord bridge (BRG-UI-001), reconciled at the wave-35 integration with the routes of
// crates/sr-api/src/routes/guilds/bridge.rs: `GET .../bridge/status`, maps addressed by their own id, the test with
// no body, the token revoke answering a teardown summary. The wire shapes (`Wire*`) are turned into the small view
// model the screen draws (`BridgeStatus`) in this one file.
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
  /** The map's own id: the PATCH, DELETE and test routes address it. */
  id: string;
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
  /** Why the bot is not connected (`state: 'error'`): Discord refused the token, or the connection dropped. */
  problem?: 'token_invalid' | 'disconnected' | null;
  /** Webhooks a past teardown could not delete on Discord (the owner removes them by hand). */
  leftovers?: { webhook_id: string; discord_channel_id: string }[];
  maps: BridgeMap[];
}

// ---- the wire (crates/sr-api/src/routes/guilds/bridge.rs)

export interface WireBridge {
  discord_guild_id: string;
  discord_guild_name: string;
  bot_user_id: string;
  bot_name: string;
  mode: string;
  status: 'connecting' | 'connected' | 'disconnected' | 'token_invalid';
  /** The gateway intents Discord granted (bit 15 = MESSAGE_CONTENT); 0 until the shard's first READY. */
  intents_granted: number;
  created_by: string;
  created_at: number;
  invite_url: string;
}

export interface WireMap {
  id: string;
  channel_id: string;
  discord_channel_id: string;
  discord_channel_name: string;
  direction: BridgeDirection;
  has_webhook: boolean;
  queued: number;
  dead: number;
  created_at: number;
}

export interface WireStatus {
  platform_on: boolean;
  module_on: boolean;
  connected: boolean;
  bridge?: WireBridge;
  maps: WireMap[];
  queued: number;
  dead_letters: number;
  leftovers: { webhook_id: string; discord_channel_id: string }[];
  is_owner: boolean;
}

/** `DELETE .../bridge/token`: the bridge was torn down (the module stays on). */
export interface TokenRevokeSummary {
  maps_removed: number;
  webhooks_deleted: number;
  leftovers: { webhook_id: string; discord_channel_id: string }[];
}

/** Discord's MESSAGE_CONTENT gateway intent. */
const MESSAGE_CONTENT = 1 << 15;

export function toStatus(w: WireStatus): BridgeStatus {
  const b = w.bridge;
  const maps = w.maps.map((m) => ({
    id: m.id,
    channel_id: m.channel_id,
    discord_channel_id: m.discord_channel_id,
    discord_channel_name: m.discord_channel_name || null,
    direction: m.direction,
  }));
  if (!b) return { state: 'no_token', token_set: false, maps, leftovers: w.leftovers };
  const state: BridgeState =
    b.status === 'connected' ? 'connected' : b.status === 'connecting' ? 'connecting' : 'error';
  return {
    state,
    token_set: true,
    discord_guild_id: b.discord_guild_id,
    discord_guild_name: b.discord_guild_name || null,
    // Known only once the shard has connected (READY tells which intents were granted).
    message_content_intent:
      b.intents_granted > 0 ? (b.intents_granted & MESSAGE_CONTENT) !== 0 : undefined,
    invite_url: b.invite_url,
    problem: state === 'error' ? (b.status as 'token_invalid' | 'disconnected') : null,
    leftovers: w.leftovers,
    maps,
  };
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

export const getBridge = async (guildId: string) =>
  toStatus(await apiFetch<WireStatus>(path(guildId, '/status'), { retries: 0 }));

/** Connect the bot (owner only); the answer is the bridge, so the screen reads the full status again. */
export const putToken = async (guildId: string, token: string) => {
  await send<WireBridge>(path(guildId, '/token'), 'PUT', { token });
  return getBridge(guildId);
};

/** Revoke the token (owner only): the bridge is torn down, the module stays on. */
export const deleteToken = (guildId: string) =>
  send<TokenRevokeSummary>(path(guildId, '/token'), 'DELETE');

export const listDiscordChannels = (guildId: string) =>
  apiFetch<DiscordChannel[]>(path(guildId, '/discord-channels'), { retries: 0 });

export const createMap = (
  guildId: string,
  body: { channel_id: string; discord_channel_id: string; direction: BridgeDirection },
) => send<WireMap>(path(guildId, '/maps'), 'POST', body);

export const deleteMap = (guildId: string, mapId: string) =>
  send<void>(path(guildId, `/maps/${mapId}`), 'DELETE');

/** "Post a hello" through the map's webhook; no body, 204 when Discord took it. */
export const testMap = (guildId: string, mapId: string) =>
  send<void>(path(guildId, `/maps/${mapId}/test`), 'POST');

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

// ---- the Discord side as members (owner direction: Discord should feel like part of SIDE ROOM)
//
// SHAPES DEFINED HERE FOR bridge-gateway TO RECONCILE (BRG-GW-001):
//  * `GET /guilds/{g}/bridge/members` (any member of the server; [] while the bridge is off) -> BridgedMember[].
//  * `BridgedMember.id` is the Discord user id as a decimal string, so the standard mention token `<@id>` carries it
//    both ways (the markdown parser already reads `<@digits>`; the web resolves it to the bridged member).
//  * An inbound message's `sr_bridge.member_id` is that same id.
//  * A Discord typer arrives as `TYPING_START` with `user_id` = that id.
//  * `avatar` is a same-origin `/cdn/external/...` path or null (a Discord URL is never loaded directly).

export type BridgedPresence = 'online' | 'idle' | 'dnd' | 'offline';

export interface BridgedMember {
  id: string;
  display_name: string;
  avatar: string | null;
  /** The member's top Discord role, for the group colour and the popout. */
  role_name: string | null;
  /** 24-bit colour of that role; 0 or null = none. */
  role_color: number | null;
  bot: boolean;
  /** Present only while the bot's presence intent knows it. */
  presence: BridgedPresence | null;
}

export const listBridgedMembers = (guildId: string) =>
  apiFetch<BridgedMember[]>(path(guildId, '/members'), { retries: 0 });
