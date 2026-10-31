// REST helpers for the Discord bridge (BRG-UI-001), reconciled at the wave-35 integration with the routes of
// crates/sr-api/src/routes/guilds/bridge.rs: `GET .../bridge/status`, maps addressed by their own id, the test with
// no body, the token revoke answering a teardown summary. The wire shapes (`Wire*`) are turned into the small view
// model the screen draws (`BridgeStatus`) in this one file.
//
// Owner-only calls (module switch, token) pass through `withReauth` because the server demands the MFA step for them
// (D-381); the mapping, test and status calls are MANAGE_GUILD and answer 404 while the module is off.
import { apiFetch } from '../../../api/client';
import { withReauth } from '../../../admin/reauth';
import { t } from '../../../i18n';
import type { GuildFeature } from '../featuresApi';
import { PERM, PERMISSIONS, toBig } from '../permissions';

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
// Wave 35b (BRG-SYN-004): the whole-server mirror, reconciled at the integration with the routes of
// crates/sr-api/src/routes/guilds/bridge_sync.rs (plan, mode, sync, reconcile, pause, holds, preflight, mirrors,
// teardown) and bridge_conflicts.rs (the conflict log). Paths are relative to `/guilds/{g}/bridge`. Classes:
//   bridge_admin  GET preflight, POST plan, GET/PATCH plan/{id}, GET sync, POST reconcile|pause|resume, GET holds,
//                 POST holds/{channel}/restore|delete (+ MANAGE_CHANNELS), GET conflicts,
//                 POST conflicts/{id}/{dismiss|resolve|take-discord|unfreeze|retry}
//   guild_owner   POST mode, GET teardown-preview, POST teardown (all with the MFA step)
//   membership    GET mirrors (badges), GET members (with display-only `role_ids` in whole-server mode)
// The server's wire shapes (`Wire*`) are turned into the view models the screens draw in this file only, as wave 35
// did for the status. Times on the wire are Unix milliseconds; the view models use seconds.
// =====================================================================================================================

const secs = (ms: number | null | undefined) => (ms ? Math.floor(ms / 1000) : null);

/** Permission ids (`SEND_MESSAGES`...) of a decimal permission word; bits SIDE ROOM does not define are left out. */
export const permissionIdsOf = (word: string | null | undefined): string[] => {
  const bits = toBig(word);
  return PERMISSIONS.filter((p) => (bits & PERM[p.id]!) === PERM[p.id]).map((p) => p.id);
};

const labelsOf = (word: string | null | undefined) => {
  const ids = permissionIdsOf(word);
  return ids.length === 0
    ? t('settings.bridgeSync.integ.none')
    : ids.map((id) => PERMISSIONS.find((p) => p.id === id)?.label ?? id).join(', ');
};

// ---- preflight (design 9.2)

/** One line of the preflight checklist. A `blocker` that is not ok stops the plan; the rest warn. */
export type PreflightId =
  'bot_admin' | 'bot_top_role' | 'intents' | 'caps' | 'unique_guild' | 'other_integration';

export interface PreflightCheck {
  id: PreflightId;
  ok: boolean;
  blocker: boolean;
  /** Plain detail when not ok (for caps: which cap). */
  detail?: string | null;
}

export interface Preflight {
  checks: PreflightCheck[];
  /** The bot's invite link for whole-server mode (Administrator), shown when a check fails. */
  invite_url?: string | null;
}

/** `GET .../preflight`: runs the checks now ("Check again"); the wire shape is the view model. */
export const getPreflight = (guildId: string) =>
  apiFetch<Preflight>(path(guildId, '/preflight'), { retries: 0 });

// ---- the plan (design 9.3 to 9.5)

export type SeedStructure = 'sr' | 'discord';
export type PlanAction = 'match' | 'pinned' | 'create_here' | 'create_there';
export type PlanObjectKind = 'role' | 'category' | 'channel' | 'thread';

export interface WirePlanItem {
  kind: PlanObjectKind;
  action: 'match' | 'pinned' | 'create_on_discord' | 'create_here';
  sr_id?: string | null;
  discord_id?: string | null;
  name: string;
  channel_type?: number | null;
  /** Roles: the shared word SIDE ROOM will push (decimal string). */
  sr_permissions: string;
  /** Roles: the word Discord holds now. */
  discord_permissions: string;
  /** Roles that exist only on Discord: the dangerous bits of the word that would be imported. */
  dangerous_bits: string;
  import: boolean;
  degraded?: string | null;
}

export interface WirePlan {
  id: string;
  status: 'building' | 'ready' | 'failed' | 'confirmed' | 'expired' | 'superseded';
  seed_structure: SeedStructure;
  plan_hash: string;
  /** Unix ms. */
  expires_at: number;
  blockers: { code: string; message: string }[];
  items: WirePlanItem[];
  not_mirrored: {
    kind: PlanObjectKind;
    sr_id?: string | null;
    discord_id?: string | null;
    name: string;
    reason: string;
    code: string;
  }[];
  estimated_writes: number;
}

export interface PlanRow {
  /** The item's index in the plan (the PATCH addresses it). */
  id: string;
  kind: PlanObjectKind;
  /** SIDE ROOM name (for a Discord-only object that will be created here, its Discord name). */
  name: string;
  discord_name?: string | null;
  action: PlanAction;
  /** Why a pair is special, shown small (a type Discord will degrade). */
  note?: string | null;
  sr_id?: string | null;
  discord_id?: string | null;
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
  'bot_admin' | 'bot_top_role' | 'intents' | 'caps' | 'unique_guild' | 'staff_guild';

export interface Plan {
  plan_id: string;
  status: 'building' | 'ready' | 'failed';
  /** Sent back with the confirmation; the server recomputes both sides' hashes (409 BRIDGE_PLAN_STALE). */
  plan_hash: string;
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

const BLOCKER_CODES: Record<string, PlanBlockerCode> = {
  bot_not_admin: 'bot_admin',
  bot_role_not_top: 'bot_top_role',
  intents_missing: 'intents',
  role_cap: 'caps',
  channel_cap: 'caps',
  discord_guild_taken: 'unique_guild',
  staff_guild: 'staff_guild',
};

const REASONS: Record<string, NotMirroredReason> = {
  managed_role: 'managed_role',
  dm: 'dm',
  e2ee: 'e2ee',
  degraded_type: 'degraded_type',
  staff_guild: 'degraded_type',
};

/** The server's plan as the plan screen draws it (rows by kind, Discord changes and imports in words). */
export function toPlan(w: WirePlan): Plan {
  const rows: PlanRow[] = w.items.map((i, n) => ({
    id: String(n),
    kind: i.kind,
    name: i.name,
    discord_name: i.action === 'match' || i.action === 'pinned' ? i.name : null,
    action: i.action === 'create_on_discord' ? 'create_there' : i.action,
    note: i.degraded ?? null,
    sr_id: i.sr_id ?? null,
    discord_id: i.discord_id ?? null,
  }));
  const changes: PlanDiscordChange[] = [];
  const imports: PlanImport[] = [];
  w.items.forEach((i, n) => {
    if (i.kind !== 'role') return;
    if (i.action === 'match' || i.action === 'pinned') {
      const sr = toBig(i.sr_permissions);
      const d = toBig(i.discord_permissions);
      const gains = (sr & ~d).toString();
      const loses = (d & ~sr).toString();
      if (gains !== '0' && permissionIdsOf(gains).length > 0)
        changes.push({
          id: `${n}+`,
          target: 'role',
          name: i.name,
          text: t('settings.bridgeSync.integ.changeGains', { name: i.name, list: labelsOf(gains) }),
        });
      if (loses !== '0' && permissionIdsOf(loses).length > 0)
        changes.push({
          id: `${n}-`,
          target: 'role',
          name: i.name,
          text: t('settings.bridgeSync.integ.changeLoses', { name: i.name, list: labelsOf(loses) }),
        });
    } else if (i.action === 'create_here') {
      imports.push({
        id: String(n),
        target: 'role',
        name: i.name,
        permissions: permissionIdsOf(i.discord_permissions),
        dangerous: permissionIdsOf(i.dangerous_bits),
        import: i.import,
      });
    }
  });
  const status: Plan['status'] =
    w.status === 'building'
      ? 'building'
      : w.status === 'failed' || w.status === 'superseded'
        ? 'failed'
        : 'ready';
  return {
    plan_id: w.id,
    status,
    plan_hash: w.plan_hash,
    expires_at: Math.floor(w.expires_at / 1000),
    seed_structure: w.seed_structure,
    blockers: w.blockers.map((b) => ({ code: BLOCKER_CODES[b.code] ?? 'caps', detail: null })),
    roles: rows.filter((r) => r.kind === 'role'),
    categories: rows.filter((r) => r.kind === 'category'),
    channels: rows.filter((r) => r.kind === 'channel'),
    threads: rows.filter((r) => r.kind === 'thread'),
    discord_changes: changes,
    imports,
    not_mirrored: w.not_mirrored.map((x, n) => ({
      id: `nm${n}`,
      name: x.name,
      reason: REASONS[x.code] ?? 'degraded_type',
    })),
    // Structural writes are paced at two a second by default (`bridge.struct_writes_per_sec`).
    estimate: { discord_calls: w.estimated_writes, seconds: Math.ceil(w.estimated_writes / 2) },
  };
}

/** `POST .../plan`: builds the plan (a dry run that writes nothing); the answer is its id. Prepared by `bridge_admin`. */
export const createPlan = (guildId: string, seed_structure: SeedStructure) =>
  apiFetch<{ plan_id: string }>(path(guildId, '/plan'), {
    method: 'POST',
    retries: 0,
    json: { seed_structure },
  });

export const getPlan = async (guildId: string, planId: string) =>
  toPlan(await apiFetch<WirePlan>(path(guildId, `/plan/${planId}`), { retries: 0 }));

export type PlanEdit =
  | { op: 'unlink'; row_id: string }
  | { op: 'link'; kind: PlanObjectKind; sr_id: string; discord_id: string }
  | { op: 'import'; row_id: string; import: boolean };

/** The PATCH body the server reads: rows are addressed by their index in the plan. */
export function planEditBody(edit: PlanEdit): Record<string, unknown> {
  switch (edit.op) {
    case 'unlink':
      return { op: 'unlink', index: Number(edit.row_id) };
    case 'import':
      return { op: 'import', index: Number(edit.row_id), import: edit.import };
    case 'link':
      return { op: 'link', kind: edit.kind, sr_id: edit.sr_id, discord_id: edit.discord_id };
  }
}

/** `PATCH .../plan/{id}`: unlink a proposed pair, link two unmatched objects, toggle an import row. */
export const editPlan = async (guildId: string, planId: string, edit: PlanEdit) =>
  toPlan(
    await apiFetch<WirePlan>(path(guildId, `/plan/${planId}`), {
      method: 'PATCH',
      retries: 0,
      json: planEditBody(edit),
    }),
  );

/**
 * `POST .../mode` (owner + MFA). Whole server needs the plan; back to selective needs none and deletes nothing.
 * A stale plan answers 409 whose message starts with `BRIDGE_PLAN_STALE`.
 */
export const setMode = (
  guildId: string,
  body: { mode: 'whole_guild'; plan_id: string; plan_hash: string } | { mode: 'selective' },
) => send<{ mode: string; sync_state: string }>(path(guildId, '/mode'), 'POST', body);

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

// ---- sync status (design 15)

export type SyncState =
  'seeding' | 'live' | 'paused' | 'paused_discord_access' | 'paused_invalid_requests';

export interface WireSyncStatus {
  mode: string;
  sync_state: SyncState | 'off';
  last_reconcile_at: number | null;
  queue_pending: number;
  queue_running: number;
  dead_ops: number;
  role_maps: number;
  channel_maps: number;
  degraded: number;
  frozen: number;
  held_deletions: number;
  intents_ok: boolean;
  reconcile_available_at: number | null;
}

export interface SyncStatus {
  mode: BridgeMode;
  sync_state: SyncState;
  bot: { administrator: boolean; top_role: boolean; intents: boolean };
  /** Unix seconds, null before the first reconcile. */
  last_reconcile_at: number | null;
  /** "Reconcile now" is allowed once per 5 minutes; this is when it is next (Unix seconds). */
  reconcile_available_at: number | null;
  queue_depth: number;
  dead_ops: number;
  /** Seconds the queue is waiting on a Discord rate limit now; 0 when not (not reported yet). */
  rate_limit_wait_secs: number;
  /** Set while a breaker or a lost permission paused the sync. */
  resume_at?: number | null;
  counts: { mapped: number; one_sided: number; degraded: number; frozen: number };
  seed?: { done: number; total: number; eta_secs: number } | null;
}

/**
 * The status page's view. The bot's Administrator and top role are checked every minute and pause the sync the
 * moment either is lost (`paused_discord_access`), so "not paused for access" is "both hold".
 */
export function toSyncStatus(w: WireSyncStatus): SyncStatus {
  const access = w.sync_state !== 'paused_discord_access';
  return {
    mode: w.mode === 'whole_guild' ? 'whole_guild' : 'selective',
    sync_state: w.sync_state === 'off' ? 'paused' : w.sync_state,
    bot: { administrator: access, top_role: access, intents: w.intents_ok },
    last_reconcile_at: secs(w.last_reconcile_at),
    reconcile_available_at: secs(w.reconcile_available_at),
    queue_depth: w.queue_pending + w.queue_running,
    dead_ops: w.dead_ops,
    rate_limit_wait_secs: 0,
    resume_at: null,
    counts: {
      mapped: w.role_maps + w.channel_maps,
      one_sided: w.held_deletions,
      degraded: w.degraded,
      frozen: w.frozen,
    },
    seed: null,
  };
}

export const getSyncStatus = async (guildId: string) =>
  toSyncStatus(await apiFetch<WireSyncStatus>(path(guildId, '/sync'), { retries: 0 }));
export const reconcileNow = (guildId: string) => send<void>(path(guildId, '/reconcile'), 'POST');
export const pauseSync = (guildId: string) => send<void>(path(guildId, '/pause'), 'POST');
export const resumeSync = (guildId: string) => send<void>(path(guildId, '/resume'), 'POST');

// ---- the conflict log (design 5.3, 11.4, 15)

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
  'apply_on_sr' | 'take_discord' | 'restore' | 'delete_now' | 'unfreeze' | 'retry' | 'dismiss';

export interface WireConflict {
  id: string;
  /** The server calls a held deletion `deletion_held`. */
  kind: string;
  object_kind: string;
  sr_object_id?: string;
  discord_object_id?: string;
  target_id?: string;
  field: string;
  discord_value?: unknown;
  sr_value?: unknown;
  discord_actor_id?: string;
  discord_actor_name?: string;
  count: number;
  status: 'open' | 'resolved' | 'dismissed' | 'superseded';
  resolution?: string;
  resolved_at?: number;
  first_at: number;
  last_at: number;
  object_name?: string;
}

export interface Conflict {
  id: string;
  kind: ConflictKind;
  object_kind: PlanObjectKind | 'overwrite' | 'op' | 'guild';
  /** SIDE ROOM id of the object (role id, channel id) when it still exists. */
  object_id?: string | null;
  object_name: string;
  /** The field that differed ("permissions", "position", "name"). */
  field?: string | null;
  /** What Discord had, in words, and what SIDE ROOM has. */
  discord_value?: string | null;
  sr_value?: string | null;
  /** Permission ids Discord's attempt carried (prefills "Apply on SIDE ROOM"); roles only. */
  discord_permissions?: string[] | null;
  /** The Discord user who did it, when the audit log named them within 10 s. */
  discord_actor?: string | null;
  /** What the bridge did about it. */
  bridge_did: string;
  count: number;
  /** Unix seconds. */
  first_at: number;
  last_at: number;
  resolved_at?: number | null;
}

const KIND_OF: Record<string, ConflictKind> = {
  permission_reverted: 'permission_reverted',
  overwrite_trimmed: 'overwrite_trimmed',
  structure_conflict: 'structure_conflict',
  frozen: 'frozen',
  deletion_held: 'held_deletion',
  role_deletion_refused: 'role_deletion_refused',
  dead_op: 'dead_op',
  cap: 'cap',
};

function bridgeDid(k: ConflictKind): string {
  switch (k) {
    case 'permission_reverted':
      return t('settings.bridgeSync.integ.didRevert');
    case 'overwrite_trimmed':
      return t('settings.bridgeSync.integ.didTrim');
    case 'structure_conflict':
      return t('settings.bridgeSync.integ.didStructure');
    case 'frozen':
      return t('settings.bridgeSync.integ.didFrozen');
    case 'held_deletion':
      return t('settings.bridgeSync.integ.didHeld');
    case 'role_deletion_refused':
      return t('settings.bridgeSync.integ.didRoleKept');
    case 'dead_op':
      return t('settings.bridgeSync.integ.didDeadOp');
    case 'cap':
      return t('settings.bridgeSync.integ.didCap');
  }
}

/** A conflict value in words: permission words become names, overwrites "allows ...; denies ...". */
function valueText(field: string, v: unknown): string | null {
  if (v === undefined || v === null) return null;
  if (field === 'permissions' && typeof v === 'string') return labelsOf(v);
  if (typeof v === 'object' && v && 'allow' in v && 'deny' in v) {
    const o = v as { allow: string; deny: string };
    return t('settings.bridgeSync.integ.allowDeny', {
      allow: labelsOf(o.allow),
      deny: labelsOf(o.deny),
    });
  }
  if (field === 'position') return t('settings.bridgeSync.integ.order');
  if (typeof v === 'string') return v;
  if (typeof v === 'object' && v && 'hours' in v) return null;
  return JSON.stringify(v);
}

export function toConflict(w: WireConflict): Conflict {
  const kind = KIND_OF[w.kind] ?? 'cap';
  return {
    id: w.id,
    kind,
    object_kind: w.object_kind as Conflict['object_kind'],
    object_id: w.sr_object_id ?? null,
    object_name: w.object_name ?? w.field ?? w.object_kind,
    field: w.field || null,
    discord_value: valueText(w.field, w.discord_value),
    sr_value: valueText(w.field, w.sr_value),
    discord_permissions:
      w.field === 'permissions' && typeof w.discord_value === 'string'
        ? permissionIdsOf(w.discord_value)
        : null,
    discord_actor: w.discord_actor_name ?? w.discord_actor_id ?? null,
    bridge_did: bridgeDid(kind),
    count: w.count,
    first_at: Math.floor(w.first_at / 1000),
    last_at: Math.floor(w.last_at / 1000),
    resolved_at: w.status === 'open' ? null : (secs(w.resolved_at) ?? Math.floor(w.last_at / 1000)),
  };
}

export const listConflicts = async (
  guildId: string,
  filter: { kind?: ConflictKind; open?: boolean } = {},
) => {
  const q = new URLSearchParams();
  if (filter.kind) q.set('kind', filter.kind === 'held_deletion' ? 'deletion_held' : filter.kind);
  q.set('status', filter.open ? 'open' : 'all');
  const list = await apiFetch<{ conflicts: WireConflict[]; open: number }>(
    path(guildId, `/conflicts?${q.toString()}`),
    { retries: 0 },
  );
  return list.conflicts.map(toConflict);
};

/**
 * Run a conflict action as the clicking person, through the normal SIDE ROOM checks (403 when they lack the
 * permission the edit needs). "Restore" and "Delete now" are the held channel's routes (`MANAGE_CHANNELS`);
 * `apply_on_sr` has no call: it opens the role editor prefilled and the person saves an ordinary edit.
 */
export const runConflictAction = (
  guildId: string,
  c: Pick<Conflict, 'id' | 'object_id'>,
  action: Exclude<ConflictAction, 'apply_on_sr'>,
) => {
  if (action === 'restore' || action === 'delete_now') {
    const what = action === 'restore' ? 'restore' : 'delete';
    return send<void>(path(guildId, `/holds/${c.object_id ?? ''}/${what}`), 'POST');
  }
  if (action === 'take_discord')
    return send<void>(path(guildId, `/conflicts/${c.id}/take-discord`), 'POST', {});
  return send<void>(path(guildId, `/conflicts/${c.id}/${action}`), 'POST');
};

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

// ---- badges (`GET .../mirrors`, any member of the server)

export interface MirrorRole {
  role_id: string;
  discord_role_id: string;
  /** Labels of the permissions Discord has on this role that SIDE ROOM has no bit for (Discord-only). */
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

export interface WireMirrors {
  mode: string;
  roles: { role_id: string; discord_role_id: string; discord_only_bits: number[]; state: string }[];
  channels: {
    channel_id: string;
    discord_channel_id: string;
    state: string;
    degraded_reason?: string | null;
  }[];
}

export function toMirrors(w: WireMirrors): Mirrors {
  return {
    mode: w.mode === 'whole_guild' ? 'whole_guild' : 'selective',
    roles: w.roles.map((r) => ({
      role_id: r.role_id,
      discord_role_id: r.discord_role_id,
      discord_only: r.discord_only_bits.map((n) =>
        t('settings.bridgeSync.integ.discordBit', { n }),
      ),
      state: r.state === 'frozen' ? 'frozen' : 'mapped',
    })),
    channels: w.channels.map((c) => ({
      channel_id: c.channel_id,
      discord_channel_id: c.discord_channel_id,
      state: c.state === 'frozen' ? 'frozen' : c.state === 'held' ? 'held' : 'mapped',
      degraded_reason: c.degraded_reason ?? null,
    })),
  };
}

/** `GET .../mirrors` (any member): which roles and channels are mirrored, for the badges. 404 while the bridge is off. */
export const getMirrors = async (guildId: string) =>
  toMirrors(await apiFetch<WireMirrors>(path(guildId, '/mirrors'), { retries: 0 }));

// ---- teardown options (design 12)

export interface TeardownPreview {
  /** Discord objects the bridge created there (`origin = 'sr'`). */
  created: { roles: number; categories: number; channels: number; threads: number };
  /** Of those channels, how many hold messages posted natively by Discord members. */
  channels_with_discord_messages: number;
  /** Whether a snapshot of Discord's permissions from before the bridge exists. */
  snapshot_available: boolean;
}

export interface WireTeardownPreview {
  whole_guild: boolean;
  created_roles: number;
  created_categories: number;
  created_channels: number;
  created_threads: number;
  created_with_discord_messages: number;
  restorable_roles: number;
  restorable_channels: number;
}

export const toTeardownPreview = (w: WireTeardownPreview): TeardownPreview => ({
  created: {
    roles: w.created_roles,
    categories: w.created_categories,
    channels: w.created_channels,
    threads: w.created_threads,
  },
  channels_with_discord_messages: w.created_with_discord_messages,
  snapshot_available: w.restorable_roles + w.restorable_channels > 0,
});

/** `GET .../teardown-preview` (owner + MFA): the counts the dialog shows before the owner chooses. */
export const getTeardownPreview = async (guildId: string) =>
  toTeardownPreview(await send<WireTeardownPreview>(path(guildId, '/teardown-preview'), 'GET'));

export interface TeardownOptions {
  /** Also delete what the bridge created on Discord. Off by default. */
  remove_created: boolean;
  /** Also put Discord's roles and overwrites back as they were before the bridge. Off by default. */
  restore_permissions: boolean;
}

export interface WireTeardownResult {
  maps_removed: number;
  webhooks_deleted: number;
  roles_removed: number;
  channels_removed: number;
  roles_restored: number;
  channels_restored: number;
  failed_ids: string[];
  leftover_webhooks: string[];
}

/**
 * Switch off with the whole-server options. ONE contract: the options go to `POST .../teardown` (owner + MFA),
 * which tears the bridge down with them; the module switch that follows then has nothing left to tear down. The
 * report the dialog shows is the teardown's (maps, webhooks Discord would not delete, removals it refused).
 */
export const switchBridgeOff = async (guildId: string, options?: TeardownOptions) => {
  if (!options) return switchBridge(guildId, false);
  const r = await send<WireTeardownResult>(path(guildId, '/teardown'), 'POST', options);
  const feature = await switchBridge(guildId, false);
  return {
    ...feature,
    teardown: {
      maps_removed: r.maps_removed,
      failed_webhooks: r.leftover_webhooks,
      residue: r.failed_ids,
    },
  } satisfies BridgeFeature;
};
