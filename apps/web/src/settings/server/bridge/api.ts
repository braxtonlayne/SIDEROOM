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

export type BridgeMode = 'selective' | 'whole_guild';

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
  /** Wave 35b: `selective` ("Only bridge these") unless the owner switched to the whole server. */
  mode: BridgeMode;
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
  if (!b)
    return { state: 'no_token', mode: 'selective', token_set: false, maps, leftovers: w.leftovers };
  const state: BridgeState =
    b.status === 'connected' ? 'connected' : b.status === 'connecting' ? 'connecting' : 'error';
  return {
    state,
    mode: b.mode === 'whole_guild' ? 'whole_guild' : 'selective',
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
  /** Whole-server removals Discord refused (ids only); the owner deletes them on Discord by hand. */
  residue?: string[];
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
  /**
   * Whole-server mode (35b): the SIDE ROOM role ids this member's Discord roles map to, for DISPLAY only (grouping,
   * colour). Absent in selective mode. Never an authorization input.
   */
  role_ids?: string[];
}

export const listBridgedMembers = (guildId: string) =>
  apiFetch<BridgedMember[]>(path(guildId, '/members'), { retries: 0 });

// =====================================================================================================================
// Wave 35b (BRG-SYN-004): the whole-server mirror. SHAPES DEFINED HERE FOR THE INTEGRATOR TO RECONCILE with the routes
// of sync-core (plan, mode, sync status, reconcile, pause) and sync-perms (conflicts); the MSW handlers of the tests
// use exactly these. Paths are relative to `/guilds/{g}/bridge`. Classes (design section 16.3):
//   bridge_admin  GET preflight, POST plan, GET/PATCH plan/{id}, GET sync, POST reconcile|pause|resume,
//                 GET conflicts, POST conflicts/{id}/{action}, GET teardown-preview
//   guild_owner   POST mode (+ the MFA step), PATCH sr/features/bridge.enabled (a `teardown` body in whole-server mode)
//   any member    GET mirrors (badges), GET members (with `role_ids` in whole-server mode)
// =====================================================================================================================

/** One line of the preflight checklist (design 9.2). A `blocker` that is not ok stops the plan; the rest warn. */
export type PreflightId =
  | 'bot_admin'
  | 'bot_top_role'
  | 'intents'
  | 'caps'
  | 'unique_guild'
  | 'other_integration';

export interface PreflightCheck {
  id: PreflightId;
  ok: boolean;
  blocker: boolean;
  /** Plain detail when not ok (for caps: "Roles after creation: 262 of 250"). */
  detail?: string | null;
}

export interface Preflight {
  checks: PreflightCheck[];
  /** The bot's invite link for whole-server mode (ADMINISTRATOR), shown when a check fails. */
  invite_url?: string | null;
}

/** `GET .../preflight`: runs the checks now ("Check again"). */
export const getPreflight = (guildId: string) =>
  apiFetch<Preflight>(path(guildId, '/preflight'), { retries: 0 });

export type SeedStructure = 'sr' | 'discord';
export type PlanAction = 'match' | 'pinned' | 'create_here' | 'create_there';
export type PlanObjectKind = 'role' | 'category' | 'channel' | 'thread';

export interface PlanRow {
  id: string;
  kind: PlanObjectKind;
  /** SIDE ROOM name (for a Discord-only object that will be created here, its Discord name). */
  name: string;
  discord_name?: string | null;
  action: PlanAction;
  /** Why a pair was matched, shown small ("same name"). */
  note?: string | null;
}

/** A change Discord will get on matched objects, in plain words ("Role Mods on Discord loses Ban Members"). */
export interface PlanDiscordChange {
  id: string;
  target: 'role' | 'channel';
  name: string;
  text: string;
}

/** Permissions of an object that exists only on Discord; the owner chooses whether they come across (Q4). */
export interface PlanImport {
  id: string;
  target: 'role' | 'channel';
  name: string;
  /** Permission ids (`SEND_MESSAGES`...) Discord has set on it. */
  permissions: string[];
  /** The subset shown red: ADMINISTRATOR, MANAGE_*, BAN/KICK/MODERATE_MEMBERS, MENTION_EVERYONE. */
  dangerous: string[];
  import: boolean;
}

export type NotMirroredReason =
  | 'e2ee'
  | 'dm'
  | 'sr_only_bit'
  | 'member_overwrite'
  | 'managed_role'
  | 'degraded_type'
  | 'cap'
  | 'voice_media';

export interface PlanNotMirrored {
  id: string;
  name: string;
  reason: NotMirroredReason;
}

export type PlanBlockerCode =
  | 'bot_admin'
  | 'bot_top_role'
  | 'intents'
  | 'caps'
  | 'unique_guild'
  | 'staff_guild';

export interface Plan {
  plan_id: string;
  status: 'building' | 'ready' | 'failed';
  /** Sent back with the confirmation; the server recomputes both sides' hashes (409 BRIDGE_PLAN_STALE). */
  plan_hash: string;
  created_at: number;
  /** Unix seconds; a plan lives 15 minutes. */
  expires_at: number;
  seed_structure: SeedStructure;
  blockers: { code: PlanBlockerCode; detail?: string | null }[];
  roles: PlanRow[];
  categories: PlanRow[];
  channels: PlanRow[];
  threads: PlanRow[];
  discord_changes: PlanDiscordChange[];
  imports: PlanImport[];
  not_mirrored: PlanNotMirrored[];
  estimate: { discord_calls: number; seconds: number };
}

/** `POST .../plan`: builds the plan as a job; the answer is its id. Prepared by `bridge_admin`. */
export const createPlan = (guildId: string, seed_structure: SeedStructure) =>
  apiFetch<{ plan_id: string }>(path(guildId, '/plan'), {
    method: 'POST',
    retries: 0,
    json: { mode: 'whole_guild', seed_structure },
  });

export const getPlan = (guildId: string, planId: string) =>
  apiFetch<Plan>(path(guildId, `/plan/${planId}`), { retries: 0 });

export type PlanEdit =
  | { op: 'unlink'; row_id: string }
  | { op: 'link'; here_id: string; discord_id: string }
  | { op: 'import'; row_id: string; import: boolean };

/** `PATCH .../plan/{id}`: unlink a proposed pair, link two unmatched objects, toggle an import row. */
export const editPlan = (guildId: string, planId: string, edit: PlanEdit) =>
  apiFetch<Plan>(path(guildId, `/plan/${planId}`), { method: 'PATCH', retries: 0, json: edit });

/**
 * `POST .../mode` (owner + MFA). Whole server needs the plan; back to selective needs none and deletes nothing.
 * A stale plan answers 409 with code `BRIDGE_PLAN_STALE`.
 */
export const setMode = (
  guildId: string,
  body: { mode: 'whole_guild'; plan_id: string; plan_hash: string } | { mode: 'selective' },
) => send<void>(path(guildId, '/mode'), 'POST', body);

export const PLAN_STALE_CODE = 'BRIDGE_PLAN_STALE';

/** Is this error the 409 of a plan whose state changed since it was built? */
export function isPlanStale(e: unknown): boolean {
  const err = e as { status?: number; message?: string; body?: object | null };
  return (
    err?.status === 409 &&
    (JSON.stringify(err.body ?? {}).includes(PLAN_STALE_CODE) ||
      (err.message ?? '').includes(PLAN_STALE_CODE))
  );
}

export type SyncState =
  | 'seeding'
  | 'live'
  | 'paused'
  | 'paused_discord_access'
  | 'paused_invalid_requests';

export interface SyncStatus {
  mode: BridgeMode;
  sync_state: SyncState;
  bot: { administrator: boolean; top_role: boolean; intents: boolean };
  /** Unix seconds, null before the first reconcile. */
  last_reconcile_at: number | null;
  /** "Reconcile now" is allowed once per 5 minutes; this is when it is next. */
  reconcile_available_at: number | null;
  queue_depth: number;
  dead_ops: number;
  /** Seconds the queue is waiting on a Discord rate limit now; 0 when not. */
  rate_limit_wait_secs: number;
  /** Set while a breaker or a lost permission paused the sync. */
  resume_at?: number | null;
  counts: { mapped: number; one_sided: number; degraded: number; frozen: number };
  seed?: { done: number; total: number; eta_secs: number } | null;
}

export const getSyncStatus = (guildId: string) =>
  apiFetch<SyncStatus>(path(guildId, '/sync'), { retries: 0 });
export const reconcileNow = (guildId: string) => send<void>(path(guildId, '/reconcile'), 'POST');
export const pauseSync = (guildId: string) => send<void>(path(guildId, '/pause'), 'POST');
export const resumeSync = (guildId: string) => send<void>(path(guildId, '/resume'), 'POST');

export type ConflictKind =
  | 'permission_reverted'
  | 'overwrite_trimmed'
  | 'structure_conflict'
  | 'frozen'
  | 'held_deletion'
  | 'role_deletion_refused'
  | 'dead_op'
  | 'cap';

export type ConflictAction =
  | 'apply_on_sr'
  | 'take_discord'
  | 'restore'
  | 'delete_now'
  | 'unfreeze'
  | 'retry'
  | 'dismiss';

export interface Conflict {
  id: string;
  kind: ConflictKind;
  object_kind: PlanObjectKind | 'overwrite';
  /** SIDE ROOM id of the object (role id, channel id) when it still exists. */
  object_id?: string | null;
  object_name: string;
  /** The field that differed ("permissions", "position", "name"). */
  field?: string | null;
  /** What Discord had, in words ("added Ban Members"), and what SIDE ROOM has. */
  discord_value?: string | null;
  sr_value?: string | null;
  /** Permission ids Discord's attempt carried (prefills "Apply on SIDE ROOM"); roles only. */
  discord_permissions?: string[] | null;
  /** The Discord user who did it, when the audit log named them within 10 s. */
  discord_actor?: string | null;
  /** What the bridge did about it ("Put it back", "Hid the channel; deletes in 22 h"). */
  bridge_did: string;
  count: number;
  /** Unix seconds. */
  first_at: number;
  last_at: number;
  resolved_at?: number | null;
}

export const listConflicts = (
  guildId: string,
  filter: { kind?: ConflictKind; open?: boolean } = {},
) => {
  const q = new URLSearchParams();
  if (filter.kind) q.set('kind', filter.kind);
  if (filter.open !== undefined) q.set('open', String(filter.open));
  const qs = q.toString();
  return apiFetch<Conflict[]>(path(guildId, `/conflicts${qs ? `?${qs}` : ''}`), { retries: 0 });
};

/**
 * `POST .../conflicts/{id}/{action}` (action in kebab case: `take-discord`, `delete-now`). Everything but `dismiss`,
 * `retry` and `unfreeze` runs as the clicking person, through the normal SIDE ROOM checks (403 when they lack the
 * permission the edit needs). `apply_on_sr` has no call: it opens the role editor prefilled and the person saves an
 * ordinary edit.
 */
export const runConflictAction = (
  guildId: string,
  id: string,
  action: Exclude<ConflictAction, 'apply_on_sr'>,
) => send<void>(path(guildId, `/conflicts/${id}/${action.replace('_', '-')}`), 'POST');

/** The conflict actions each kind offers (design 15). */
export const CONFLICT_ACTIONS: Record<ConflictKind, ConflictAction[]> = {
  permission_reverted: ['apply_on_sr', 'dismiss'],
  overwrite_trimmed: ['dismiss'],
  structure_conflict: ['take_discord', 'dismiss'],
  frozen: ['unfreeze'],
  held_deletion: ['restore', 'delete_now'],
  role_deletion_refused: ['dismiss'],
  dead_op: ['retry', 'dismiss'],
  cap: ['dismiss'],
};

export interface MirrorRole {
  role_id: string;
  discord_role_id: string;
  /** Names of the permissions Discord has on this role that SIDE ROOM has no bit for (Discord-only); empty when none. */
  discord_only: string[];
  state: 'mapped' | 'frozen';
}

export interface MirrorChannel {
  channel_id: string;
  discord_channel_id: string;
  state: 'mapped' | 'frozen' | 'held';
  /** Set when Discord refused the real type (Community needed): the plain reason. */
  degraded_reason?: string | null;
}

export interface Mirrors {
  mode: BridgeMode;
  roles: MirrorRole[];
  channels: MirrorChannel[];
}

/** `GET .../mirrors` (any member): which roles and channels are mirrored, for the badges. 404 while the bridge is off. */
export const getMirrors = (guildId: string) =>
  apiFetch<Mirrors>(path(guildId, '/mirrors'), { retries: 0 });

// ---- teardown options (design 12)

export interface TeardownPreview {
  /** Discord objects the bridge created there (`origin = 'sr'`). */
  created: { roles: number; categories: number; channels: number; threads: number };
  /** Of those channels, how many hold messages posted natively by Discord members. */
  channels_with_discord_messages: number;
  /** Whether a snapshot of Discord's permissions from before the bridge exists. */
  snapshot_available: boolean;
}

export const getTeardownPreview = (guildId: string) =>
  apiFetch<TeardownPreview>(path(guildId, '/teardown-preview'), { retries: 0 });

export interface TeardownOptions {
  /** Also delete what the bridge created on Discord. Off by default. */
  remove_created: boolean;
  /** Also put Discord's roles and overwrites back as they were before the bridge. Off by default. */
  restore_permissions: boolean;
}

/** Switch off with the whole-server options: the body gains `teardown`; the report gains `residue` (ids only). */
export const switchBridgeOff = (guildId: string, options?: TeardownOptions) =>
  send<BridgeFeature>(
    `/guilds/${guildId}/sr/features/${encodeURIComponent(BRIDGE_MODULE_ID)}`,
    'PATCH',
    { enabled: false, ...(options ? { teardown: options } : {}) },
  );
