/**
 * Pure mapping from the normalized store (Discord-shaped, snake_case, src/store/types.ts) to the Lounge
 * view model (camelCase, src/lounge/data.ts). No React here, so it is unit-testable on its own.
 */
import type {
  Attachment,
  Activity,
  Channel,
  ChannelPermissions,
  ChannelType,
  DmChannel,
  Guild,
  GuildPermissions,
  LoungeUser,
  Member,
  Message,
  Poll,
  Presence,
  Role,
  Snowflake,
} from './data';
import { cmpId } from '../store/reducer';
import { canOpenSettingsFromBits } from '../settings/server/permissions';
import type * as S from '../store/types';
import type { PermEngine } from './permEngine';

const SNOWFLAKE_EPOCH_MS = 1420070400000n;

/** Creation time encoded in a snowflake, as ISO text; '' when the id is not a snowflake. */
export function snowflakeTime(id: string | null | undefined): string {
  if (!id || !/^\d+$/.test(id)) return '';
  return new Date(Number((BigInt(id) >> 22n) + SNOWFLAKE_EPOCH_MS)).toISOString();
}

/** Discord permission bits used by the Lounge (sr-core permissions.rs). */
export const PERM = {
  CREATE_INSTANT_INVITE: 1n << 0n,
  ADMINISTRATOR: 1n << 3n,
  MANAGE_CHANNELS: 1n << 4n,
  MANAGE_GUILD: 1n << 5n,
  SEND_MESSAGES: 1n << 11n,
  VIEW_CHANNEL: 1n << 10n,
  MANAGE_MESSAGES: 1n << 13n,
  MENTION_EVERYONE: 1n << 17n,
  MODERATE_MEMBERS: 1n << 40n,
  PIN_MESSAGES: 1n << 51n,
} as const;

const ALL_BITS = (1n << 64n) - 1n;

const toBig = (s: string | undefined): bigint => {
  try {
    return s ? BigInt(s) : 0n;
  } catch {
    return 0n;
  }
};

/**
 * Base guild permissions for the viewer: owner and Administrator get every bit; otherwise the union of
 * the @everyone role (its id is the guild id) and the viewer's roles. Channel overwrites are not applied.
 */
export function basePermissions(
  guildId: Snowflake,
  ownerId: Snowflake | undefined,
  viewerId: Snowflake,
  roleById: ReadonlyMap<Snowflake, S.Role>,
  viewerRoleIds: readonly Snowflake[] | undefined,
): bigint {
  if (ownerId && ownerId === viewerId) return ALL_BITS;
  let p = toBig(roleById.get(guildId)?.permissions);
  for (const rid of viewerRoleIds ?? []) p |= toBig(roleById.get(rid)?.permissions);
  return p & PERM.ADMINISTRATOR ? ALL_BITS : p;
}

export function toGuildPermissions(base: bigint): GuildPermissions {
  const has = (bit: bigint) => (base & bit) === bit;
  return {
    manageGuild: has(PERM.MANAGE_GUILD),
    manageChannels: has(PERM.MANAGE_CHANNELS),
    createInvite: has(PERM.CREATE_INSTANT_INVITE),
    moderate: has(PERM.MODERATE_MEMBERS),
    manageMessages: has(PERM.MANAGE_MESSAGES),
    mentionEveryone: has(PERM.MENTION_EVERYONE),
    pinMessages: has(PERM.PIN_MESSAGES),
    serverSettings: canOpenSettingsFromBits(base),
  };
}

/** Inputs for one viewer-in-one-channel computation. */
export interface ChannelPermInput {
  guildId: Snowflake;
  ownerId: Snowflake | undefined;
  viewerId: Snowflake;
  viewer: S.Member | undefined;
  roleById: ReadonlyMap<Snowflake, S.Role>;
  channel: S.Channel;
}

/**
 * The viewer's permissions in one channel via the shared engine (overwrites, timeout, implicit rules).
 * Returns `undefined` when the viewer's member object is not loaded yet, so callers keep the fallback.
 */
export function computeChannelPermissions(
  engine: PermEngine,
  i: ChannelPermInput,
): ChannelPermissions | undefined {
  const isOwner = !!i.ownerId && i.ownerId === i.viewerId;
  if (!i.viewer && !isOwner) return undefined;
  const roleIds = (i.viewer?.roles ?? []).filter((id) => id !== i.guildId);
  const until = i.viewer?.communication_disabled_until
    ? Date.parse(i.viewer.communication_disabled_until)
    : NaN;
  let bits: bigint;
  try {
    bits = BigInt(
      engine.compute({
        is_owner: isOwner,
        everyone: i.roleById.get(i.guildId)?.permissions ?? '0',
        role_permissions: roleIds.map((id) => i.roleById.get(id)?.permissions ?? '0'),
        guild_id: i.guildId,
        member_id: i.viewerId,
        member_role_ids: roleIds,
        overwrites: (i.channel.permission_overwrites ?? []).map((o) => ({
          id: o.id,
          kind: o.type === 1 ? 'member' : 'role',
          allow: o.allow,
          deny: o.deny,
        })),
        communication_disabled_until_ms: Number.isFinite(until) ? until : null,
      }),
    );
  } catch {
    return undefined;
  }
  const has = (bit: bigint) => (bits & bit) === bit;
  return {
    view: has(PERM.VIEW_CHANNEL),
    send: has(PERM.SEND_MESSAGES),
    manageMessages: has(PERM.MANAGE_MESSAGES),
    pinMessages: has(PERM.PIN_MESSAGES),
    mentionEveryone: has(PERM.MENTION_EVERYONE),
    manageChannels: has(PERM.MANAGE_CHANNELS),
    createInvite: has(PERM.CREATE_INSTANT_INVITE),
  };
}

export function toLoungeUser(u: S.User): LoungeUser {
  return {
    id: u.id,
    username: u.username,
    displayName: u.global_name || u.username,
    // Avatar hashes need a CDN URL scheme that the API does not publish yet; initials are shown instead.
    avatarUrl: null,
    ...(u.bot ? { bot: true } : {}),
  };
}

const CHANNEL_TYPE: Record<number, ChannelType> = {
  0: 'text',
  2: 'voice',
  4: 'category',
  5: 'announcement',
  13: 'stage',
  15: 'forum',
  16: 'media',
};

/** Unread = the newest message is newer than the viewer's read state (no read state = unread). */
export function isUnread(c: S.Channel, rs: S.ReadState | undefined): boolean {
  const last = c.last_message_id ?? null;
  if (!last) return false;
  const read = rs?.last_message_id ?? null;
  return !read || cmpId(last, read) > 0;
}

export function toLoungeChannel(
  c: S.Channel,
  guildId: Snowflake,
  rs: S.ReadState | undefined,
  canSend: boolean,
  permissions?: ChannelPermissions,
): Channel {
  return {
    id: c.id,
    guildId,
    type: CHANNEL_TYPE[c.type] ?? 'text',
    name: c.name ?? '',
    topic: c.topic ?? null,
    parentId: c.parent_id ?? null,
    position: c.position ?? 0,
    unread: isUnread(c, rs),
    mentionCount: rs?.mention_count ?? 0,
    lastReadMessageId: rs?.last_message_id ?? null,
    canSend: permissions ? permissions.send : canSend,
    ...(permissions ? { permissions } : {}),
    ...(c.rate_limit_per_user ? { slowmodeSeconds: c.rate_limit_per_user } : {}),
  };
}

export function toDmChannel(c: S.Channel, rs: S.ReadState | undefined): DmChannel {
  return {
    id: c.id,
    recipientIds: (c.recipients ?? []).map((u) => u.id),
    name: c.name ?? null,
    lastMessageAt: snowflakeTime(c.last_message_id),
    unread: isUnread(c, rs),
    mentionCount: rs?.mention_count ?? 0,
  };
}

export function toLoungeGuild(
  g: S.Guild,
  perms: GuildPermissions,
  unread: boolean,
  mentionCount: number,
): Guild {
  return {
    id: g.id,
    name: g.name,
    iconUrl: null,
    ownerId: g.owner_id ?? '',
    unread,
    mentionCount,
    permissions: perms,
  };
}

export function toLoungeRole(r: S.Role): Role {
  return {
    id: r.id,
    name: r.name,
    color: r.color ? `#${r.color.toString(16).padStart(6, '0')}` : null,
    hoist: !!r.hoist,
    position: r.position,
  };
}

const ACTIVITY_KIND: Record<number, Activity['kind']> = {
  0: 'playing',
  2: 'listening',
  3: 'watching',
  4: 'custom',
};

interface WireActivity {
  type?: number;
  name?: string;
  state?: string | null;
}

export function toLoungePresence(p: S.Presence | undefined): Presence {
  const status = p?.status ?? 'offline';
  const a = (p?.activities as WireActivity[] | undefined)?.find(
    (x) => x.type !== undefined && ACTIVITY_KIND[x.type] !== undefined,
  );
  if (!a || a.type === undefined) return { status };
  const text = (a.type === 4 ? (a.state ?? a.name) : a.name) ?? '';
  return text ? { status, activity: { kind: ACTIVITY_KIND[a.type]!, text } } : { status };
}

export function toLoungeMember(m: S.Member, user: LoungeUser): Member {
  return {
    user,
    guildId: m.guild_id,
    nick: m.nick ?? null,
    roleIds: m.roles,
  };
}

const FLAG_SUPPRESS_EMBEDS = 1 << 2;
const FLAG_VOICE = 1 << 13;

function decodeWaveform(b64: string | null | undefined): number[] | null {
  if (!b64) return null;
  try {
    return Array.from(atob(b64), (c) => c.charCodeAt(0));
  } catch {
    return null;
  }
}

export function toLoungeAttachment(a: S.RawAttachment, now = Date.now()): Attachment {
  return {
    id: a.id,
    filename: a.filename,
    size: a.size,
    contentType: a.content_type ?? 'application/octet-stream',
    url: a.url,
    ...(a.width ? { width: a.width } : {}),
    ...(a.height ? { height: a.height } : {}),
    // Discord marks spoiler attachments with a filename prefix.
    ...(a.filename.startsWith('SPOILER_') ? { spoiler: true } : {}),
    description: a.description ?? null,
    durationSecs: a.duration_secs ?? null,
    waveform: decodeWaveform(a.waveform),
    ...(a.expires_at && new Date(a.expires_at).getTime() <= now ? { expired: true } : {}),
  };
}

export function toLoungePoll(p: S.RawPoll): Poll {
  const counts = new Map((p.results?.answer_counts ?? []).map((c) => [c.id, c]));
  return {
    question: p.question.text ?? '',
    answers: p.answers.map((a) => ({
      id: a.answer_id,
      text: a.poll_media.text ?? '',
      emoji: a.poll_media.emoji?.name ?? undefined,
      count: counts.get(a.answer_id)?.count ?? 0,
      me: counts.get(a.answer_id)?.me_voted ?? false,
    })),
    multi: !!p.allow_multiselect,
    expiresAt: p.expiry ?? null,
    finalized: !!p.results?.is_finalized,
  };
}

/** Converts one store message into the Lounge view model. */
export function toLoungeMessage(m: S.Message): Message {
  const reactions = m.reactions?.map((r) => ({
    emoji: r.emoji.name ?? r.emoji.id ?? '',
    count: r.count,
    me: r.me,
  }));
  const snap = m.message_reference?.type === 1 ? m.message_snapshots?.[0]?.message : undefined;
  const flags = m.flags ?? 0;
  const attachments = m.attachments?.map((a) => toLoungeAttachment(a));
  return {
    id: m.id,
    channelId: m.channel_id,
    author: toLoungeUser(m.author),
    content: m.content,
    createdAt: m.timestamp,
    editedAt: m.edited_timestamp ?? null,
    // A forward also carries a message_reference, but it is not a reply.
    replyToId: m.message_reference?.type === 1 ? null : (m.message_reference?.message_id ?? null),
    ...(reactions && reactions.length ? { reactions } : {}),
    ...(attachments && attachments.length ? { attachments } : {}),
    pending: m.local?.status === 'sending',
    failed: m.local?.status === 'failed',
    ...(m.pinned ? { pinned: true } : {}),
    ...(m.type ? { type: m.type } : {}),
    mentionIds: (m.mentions ?? []).map((u) => u.id),
    mentionRoleIds: m.mention_roles ?? [],
    mentionEveryone: !!m.mention_everyone,
    ...(m.embeds?.length ? { embeds: m.embeds } : {}),
    ...(m.sticker_items?.length
      ? {
          stickers: m.sticker_items.map((s) => ({
            id: s.id,
            name: s.name,
            formatType: s.format_type,
          })),
        }
      : {}),
    ...(m.poll ? { poll: toLoungePoll(m.poll) } : {}),
    ...(m.components?.length ? { components: m.components } : {}),
    ...(snap
      ? {
          forwarded: {
            content: snap.content ?? '',
            createdAt: snap.timestamp ?? m.timestamp,
            attachments: (snap.attachments ?? []).map((a) => toLoungeAttachment(a)),
            embeds: snap.embeds ?? [],
          },
        }
      : {}),
    ...(flags & FLAG_VOICE ? { voice: true } : {}),
    ...(flags & FLAG_SUPPRESS_EMBEDS ? { suppressEmbeds: true } : {}),
  };
}
