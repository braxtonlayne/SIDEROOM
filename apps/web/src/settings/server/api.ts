// REST helpers for the server settings screens. Paths and shapes mirror crates/sr-api/src/routes/{guilds,settings}.
// Mutations never retry on 429 (retries: 0) so the user sees the error.
import { apiFetch } from '../../api/client';

const send = <T>(path: string, method: string, json?: unknown, headers?: Record<string, string>) =>
  apiFetch<T>(path, {
    method,
    retries: 0,
    ...(json !== undefined ? { json } : {}),
    ...(headers ? { headers } : {}),
  });

/** Reason header for audit-logged actions (Discord compatible). */
const reasonHeader = (reason?: string) =>
  reason ? { 'X-Audit-Log-Reason': encodeURIComponent(reason) } : undefined;

export interface UserRef {
  id: string;
  username?: string;
  global_name?: string | null;
}
export const userName = (u: UserRef | undefined | null, fallback = 'Unknown user') =>
  u ? u.global_name || u.username || `User ${u.id}` : fallback;

// ---- guild ----

export interface GuildObject {
  id: string;
  name: string;
  icon: string | null;
  banner?: string | null;
  splash?: string | null;
  description: string | null;
  owner_id: string;
  system_channel_id: string | null;
  /** Voice channel idle members move to (voice is not built yet, the setting is stored). */
  afk_channel_id?: string | null;
  /** Seconds: 60, 300, 900, 1800 or 3600. */
  afk_timeout?: number;
  verification_level: number;
  default_message_notifications: number;
  explicit_content_filter: number;
  preferred_locale: string;
  features: string[];
}
export interface ModifyGuildBody {
  name?: string;
  description?: string | null;
  system_channel_id?: string | null;
  afk_channel_id?: string | null;
  afk_timeout?: number;
  verification_level?: number;
  default_message_notifications?: number;
  explicit_content_filter?: number;
  preferred_locale?: string;
  /** Data URI to set, null to remove. */
  icon?: string | null;
  banner?: string | null;
  splash?: string | null;
}
export const AFK_TIMEOUTS = [
  { seconds: 60, label: '1 minute' },
  { seconds: 300, label: '5 minutes' },
  { seconds: 900, label: '15 minutes' },
  { seconds: 1800, label: '30 minutes' },
  { seconds: 3600, label: '1 hour' },
];
export const getGuild = (id: string) => apiFetch<GuildObject>(`/guilds/${id}`, { retries: 0 });
export const modifyGuild = (id: string, body: ModifyGuildBody) =>
  send<GuildObject>(`/guilds/${id}`, 'PATCH', body);
export const deleteGuild = (id: string) => send<void>(`/guilds/${id}`, 'DELETE');

// ---- roles ----

export interface RoleObject {
  id: string;
  name: string;
  color: number;
  hoist: boolean;
  position: number;
  permissions: string;
  managed: boolean;
  mentionable: boolean;
  icon?: string | null;
  unicode_emoji?: string | null;
}
export interface RoleBody {
  name?: string;
  permissions?: string;
  color?: number;
  hoist?: boolean;
  mentionable?: boolean;
}
export const listRoles = (g: string) =>
  apiFetch<RoleObject[]>(`/guilds/${g}/roles`, { retries: 0 });
export const createRole = (g: string, body: RoleBody) =>
  send<RoleObject>(`/guilds/${g}/roles`, 'POST', body);
export const modifyRole = (g: string, r: string, body: RoleBody) =>
  send<RoleObject>(`/guilds/${g}/roles/${r}`, 'PATCH', body);
export const deleteRole = (g: string, r: string) => send<void>(`/guilds/${g}/roles/${r}`, 'DELETE');
export const reorderRoles = (g: string, moves: { id: string; position: number }[]) =>
  send<RoleObject[]>(`/guilds/${g}/roles`, 'PATCH', moves);
export const addMemberRole = (g: string, u: string, r: string) =>
  send<void>(`/guilds/${g}/members/${u}/roles/${r}`, 'PUT');
export const removeMemberRole = (g: string, u: string, r: string) =>
  send<void>(`/guilds/${g}/members/${u}/roles/${r}`, 'DELETE');

// ---- channels ----

export interface OverwriteObject {
  id: string;
  /** 0 role, 1 member. */
  type: number;
  allow: string;
  deny: string;
}
export interface ChannelObject {
  id: string;
  type: number;
  guild_id: string | null;
  name: string | null;
  topic: string | null;
  position: number;
  parent_id: string | null;
  nsfw: boolean;
  rate_limit_per_user: number;
  user_limit?: number;
  bitrate?: number;
  /** Minutes of inactivity before new threads in this channel archive. */
  default_auto_archive_duration?: number | null;
  default_thread_rate_limit_per_user?: number | null;
  permission_overwrites: OverwriteObject[];
}
export interface ChannelBody {
  name?: string;
  type?: number;
  topic?: string | null;
  nsfw?: boolean;
  rate_limit_per_user?: number;
  position?: number;
  parent_id?: string | null;
  lock_permissions?: boolean;
  default_auto_archive_duration?: number;
  default_thread_rate_limit_per_user?: number;
}
export const listChannels = (g: string) =>
  apiFetch<ChannelObject[]>(`/guilds/${g}/channels`, { retries: 0 });
export const createChannel = (g: string, body: ChannelBody) =>
  send<ChannelObject>(`/guilds/${g}/channels`, 'POST', body);
export const modifyChannel = (c: string, body: ChannelBody) =>
  send<ChannelObject>(`/channels/${c}`, 'PATCH', body);
export const deleteChannel = (c: string) => send<ChannelObject>(`/channels/${c}`, 'DELETE');
export const reorderChannels = (
  g: string,
  moves: { id: string; position: number; parent_id?: string | null; lock_permissions?: boolean }[],
) => send<void>(`/guilds/${g}/channels`, 'PATCH', moves);
export const putOverwrite = (
  c: string,
  id: string,
  body: { type: number; allow: string; deny: string },
) => send<void>(`/channels/${c}/permissions/${id}`, 'PUT', body);
export const deleteOverwrite = (c: string, id: string) =>
  send<void>(`/channels/${c}/permissions/${id}`, 'DELETE');

// ---- members and bans ----

export interface MemberObject {
  user: UserRef;
  nick: string | null;
  roles: string[];
  joined_at: string;
  pending?: boolean;
  communication_disabled_until: string | null;
}
export const MEMBER_PAGE = 100;
/** Cursor paging by user id (`after`). There is no server-side search (see report). */
export const listMembers = (g: string, after?: string, limit = MEMBER_PAGE) =>
  apiFetch<MemberObject[]>(`/guilds/${g}/members?limit=${limit}${after ? `&after=${after}` : ''}`, {
    retries: 0,
  });
export const getMember = (g: string, u: string) =>
  apiFetch<MemberObject>(`/guilds/${g}/members/${u}`, { retries: 0 });
export const kickMember = (g: string, u: string, reason?: string) =>
  send<void>(`/guilds/${g}/members/${u}`, 'DELETE', undefined, reasonHeader(reason));
/** `until` is an ISO timestamp, or null to clear the timeout. */
export const timeoutMember = (g: string, u: string, until: string | null) =>
  send<MemberObject>(`/guilds/${g}/members/${u}`, 'PATCH', { communication_disabled_until: until });

export interface BanObject {
  reason: string | null;
  user: UserRef;
}
export const listBans = (g: string) =>
  apiFetch<BanObject[]>(`/guilds/${g}/bans?limit=1000`, { retries: 0 });
export const banMember = (
  g: string,
  u: string,
  o: { reason?: string; delete_message_seconds?: number },
) =>
  send<void>(`/guilds/${g}/bans/${u}`, 'PUT', {
    ...(o.delete_message_seconds ? { delete_message_seconds: o.delete_message_seconds } : {}),
    ...(o.reason ? { reason: o.reason } : {}),
  });
export const unban = (g: string, u: string) => send<void>(`/guilds/${g}/bans/${u}`, 'DELETE');

// ---- invites ----

export { listGuildInvites, listChannelInvites, revokeInvite, createInvite } from '../../store/rest';
export type { Invite, CreateInviteOptions } from '../../store/rest';

// ---- emoji and stickers ----

export interface EmojiObject {
  id: string;
  name: string;
  roles: string[];
  user: UserRef;
  animated: boolean;
  managed: boolean;
  available: boolean;
}
export interface StickerObject {
  id: string;
  name: string;
  description: string | null;
  tags: string;
  format_type: number;
  available: boolean;
  user: UserRef;
}
export const listEmojis = (g: string) =>
  apiFetch<EmojiObject[]>(`/guilds/${g}/emojis`, { retries: 0 });
/** `image` is a data URI (`data:image/png;base64,...`). */
export const createEmoji = (g: string, body: { name: string; image: string; roles?: string[] }) =>
  send<EmojiObject>(`/guilds/${g}/emojis`, 'POST', body);
export const modifyEmoji = (g: string, id: string, body: { name?: string; roles?: string[] }) =>
  send<EmojiObject>(`/guilds/${g}/emojis/${id}`, 'PATCH', body);
export const deleteEmoji = (g: string, id: string) =>
  send<void>(`/guilds/${g}/emojis/${id}`, 'DELETE');
export const listStickers = (g: string) =>
  apiFetch<StickerObject[]>(`/guilds/${g}/stickers`, { retries: 0 });
export const createSticker = (
  g: string,
  body: { name: string; description?: string; tags: string; file: string },
) => send<StickerObject>(`/guilds/${g}/stickers`, 'POST', body);
export const modifyStickerReq = (
  g: string,
  id: string,
  body: { name?: string; description?: string | null; tags?: string },
) => send<StickerObject>(`/guilds/${g}/stickers/${id}`, 'PATCH', body);
export const deleteSticker = (g: string, id: string) =>
  send<void>(`/guilds/${g}/stickers/${id}`, 'DELETE');

/** Slot limits are operator-configured and not exposed by the API: these are the sr-chat defaults. */
export const EXPRESSION_DEFAULTS = {
  staticEmoji: 250,
  animatedEmoji: 250,
  stickers: 60,
  maxEmojiBytes: 256 * 1024,
  maxStickerBytes: 512 * 1024,
};

// ---- AutoMod ----

export const TRIGGER = {
  KEYWORD: 1,
  SPAM: 3,
  PRESET: 4,
  MENTION_SPAM: 5,
  MEMBER_PROFILE: 6,
  LINKS: 100,
} as const;
export const ACTION = { BLOCK: 1, ALERT: 2, TIMEOUT: 3 } as const;

export interface TriggerMetadata {
  keyword_filter?: string[];
  regex_patterns?: string[];
  presets?: number[];
  allow_list?: string[];
  mention_total_limit?: number | null;
  mention_raid_protection_enabled?: boolean;
  link_block_all?: boolean;
  link_block_domains?: string[];
  link_allow_domains?: string[];
}
export interface RuleAction {
  type: number;
  metadata: { channel_id?: string; duration_seconds?: number; custom_message?: string };
}
export interface AutoModRule {
  id: string;
  guild_id: string;
  name: string;
  creator_id: string;
  event_type: number;
  trigger_type: number;
  trigger_metadata: TriggerMetadata;
  actions: RuleAction[];
  enabled: boolean;
  exempt_roles: string[];
  exempt_channels: string[];
}
export interface AutoModBody {
  name?: string;
  event_type?: number;
  trigger_type?: number;
  trigger_metadata?: TriggerMetadata;
  actions?: RuleAction[];
  enabled?: boolean;
  exempt_roles?: string[];
  exempt_channels?: string[];
}
export const listRules = (g: string) =>
  apiFetch<AutoModRule[]>(`/guilds/${g}/auto-moderation/rules`, { retries: 0 });
export const createRule = (g: string, body: AutoModBody) =>
  send<AutoModRule>(`/guilds/${g}/auto-moderation/rules`, 'POST', body);
export const modifyRule = (g: string, id: string, body: AutoModBody) =>
  send<AutoModRule>(`/guilds/${g}/auto-moderation/rules/${id}`, 'PATCH', body);
export const deleteRule = (g: string, id: string) =>
  send<void>(`/guilds/${g}/auto-moderation/rules/${id}`, 'DELETE');

// ---- audit log ----

export interface AuditChange {
  key: string;
  old_value?: unknown;
  new_value?: unknown;
}
export interface AuditEntry {
  id: string;
  user_id: string;
  target_id: string | null;
  action_type: number;
  changes: AuditChange[];
  options?: Record<string, unknown>;
  reason?: string;
}
export interface AuditLog {
  audit_log_entries: AuditEntry[];
  users: UserRef[];
}
export const AUDIT_PAGE = 50;
export function listAuditLog(
  g: string,
  q: { user_id?: string; action_type?: number; before?: string },
) {
  const p = new URLSearchParams({ limit: String(AUDIT_PAGE) });
  if (q.user_id) p.set('user_id', q.user_id);
  if (q.action_type !== undefined) p.set('action_type', String(q.action_type));
  if (q.before) p.set('before', q.before);
  return apiFetch<AuditLog>(`/guilds/${g}/audit-logs?${p}`, { retries: 0 });
}

// ---- webhooks and templates ----

export interface WebhookObject {
  id: string;
  type: number;
  guild_id: string;
  channel_id: string;
  user?: UserRef;
  name: string;
  avatar: string | null;
  /** Only in the creation response. */
  token?: string;
  url?: string;
}
export const listWebhooks = (g: string) =>
  apiFetch<WebhookObject[]>(`/guilds/${g}/webhooks`, { retries: 0 });
export const createWebhook = (channelId: string, name: string) =>
  send<WebhookObject>(`/channels/${channelId}/webhooks`, 'POST', { name });
export const deleteWebhook = (id: string) => send<void>(`/webhooks/${id}`, 'DELETE');
/** Issues a fresh token and URL (shown once) and invalidates the old one (D-164). */
export const resetWebhookToken = (id: string) =>
  send<WebhookObject>(`/webhooks/${id}/reset-token`, 'POST', {});

export interface TemplateObject {
  code: string;
  name: string;
  description: string | null;
  usage_count: number;
  creator: UserRef;
  created_at: string;
  updated_at: string;
  source_guild_id: string;
  is_dirty: boolean | null;
}
export const listTemplates = (g: string) =>
  apiFetch<TemplateObject[]>(`/guilds/${g}/templates`, { retries: 0 });
export const createTemplate = (g: string, body: { name: string; description?: string | null }) =>
  send<TemplateObject>(`/guilds/${g}/templates`, 'POST', body);
export const syncTemplate = (g: string, code: string) =>
  send<TemplateObject>(`/guilds/${g}/templates/${code}`, 'PUT');
export const deleteTemplate = (g: string, code: string) =>
  send<TemplateObject>(`/guilds/${g}/templates/${code}`, 'DELETE');

// ---- community ----

export interface CommunityObject {
  guild_id: string;
  community_enabled: boolean;
  rules_channel_id: string | null;
  public_updates_channel_id: string | null;
  safety_alerts_channel_id: string | null;
  description: string | null;
  preferred_locale: string;
  onboarding_enabled: boolean;
  welcome_screen_enabled: boolean;
  guide_enabled: boolean;
  invites_paused: boolean;
  raid_alert_joins_per_minute: number;
  age_restricted: boolean;
}
export type CommunityPatch = Partial<
  Omit<
    CommunityObject,
    | 'guild_id'
    | 'rules_channel_id'
    | 'public_updates_channel_id'
    | 'safety_alerts_channel_id'
    | 'description'
  >
> & {
  rules_channel_id?: string | null;
  public_updates_channel_id?: string | null;
  safety_alerts_channel_id?: string | null;
  description?: string | null;
};
export const getCommunity = (g: string) =>
  apiFetch<CommunityObject>(`/guilds/${g}/sr/community`, { retries: 0 });
export const modifyCommunity = (g: string, body: CommunityPatch) =>
  send<CommunityObject>(`/guilds/${g}/sr/community`, 'PATCH', body);

export interface OnboardingOption {
  id?: string | null;
  title: string;
  description?: string | null;
  emoji?: string | null;
  channel_ids: string[];
  role_ids: string[];
}
export interface OnboardingPrompt {
  id?: string | null;
  title: string;
  single_select: boolean;
  required: boolean;
  in_onboarding: boolean;
  options: OnboardingOption[];
}
export interface OnboardingObject {
  guild_id: string;
  enabled: boolean;
  default_channel_ids: string[];
  prompts: OnboardingPrompt[];
}
export const getOnboarding = (g: string) =>
  apiFetch<OnboardingObject>(`/guilds/${g}/onboarding`, { retries: 0 });
export const putOnboarding = (
  g: string,
  body: { enabled: boolean; default_channel_ids: string[]; prompts: OnboardingPrompt[] },
) => send<OnboardingObject>(`/guilds/${g}/onboarding`, 'PUT', body);

export interface WelcomeChannel {
  channel_id: string;
  description: string;
  emoji_name?: string | null;
}
export interface WelcomeScreenObject {
  enabled: boolean;
  description: string | null;
  welcome_channels: WelcomeChannel[];
}
export const getWelcomeScreen = (g: string) =>
  apiFetch<WelcomeScreenObject>(`/guilds/${g}/welcome-screen`, { retries: 0 });
export const modifyWelcomeScreen = (
  g: string,
  body: { enabled?: boolean; description?: string | null; welcome_channels?: WelcomeChannel[] },
) => send<WelcomeScreenObject>(`/guilds/${g}/welcome-screen`, 'PATCH', body);

// ---- errors ----

/** User-facing copy for an API failure (field errors first, then the message). */
export function errorText(e: unknown, fallback = 'Something went wrong. Try again.'): string {
  const err = e as {
    status?: number;
    message?: string;
    body?: { errors?: Record<string, { _errors?: { message: string }[] }> } | null;
  };
  const fields = err?.body?.errors;
  if (fields) {
    const msgs = Object.entries(fields).flatMap(([k, v]) =>
      (v._errors ?? []).map((x) => (k === '_errors' ? x.message : `${k}: ${x.message}`)),
    );
    if (msgs.length) return msgs.join(' ');
  }
  if (err?.status === 403) return 'You do not have permission to do that.';
  if (err?.status === 404) return 'That no longer exists.';
  if (err?.status === 429) return 'You are doing that too quickly. Wait a moment and try again.';
  return err?.message || fallback;
}

// ---- member prune (LNG-SSET-029) ----

export interface PruneResult {
  /** Members that would be (preview) or were (run) removed. */
  pruned: number;
}
/** How many members a prune would remove now (`KICK_MEMBERS`). */
export const pruneCount = (g: string, days: number, includeRoles: string[]) => {
  const q = new URLSearchParams({ days: String(days) });
  if (includeRoles.length) q.set('include_roles', includeRoles.join(','));
  return apiFetch<PruneResult>(`/guilds/${g}/prune?${q}`, { retries: 0 });
};
export const pruneMembers = (g: string, days: number, includeRoles: string[], reason?: string) =>
  send<PruneResult>(
    `/guilds/${g}/prune`,
    'POST',
    { days, include_roles: includeRoles },
    reasonHeader(reason),
  );
