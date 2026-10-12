/**
 * Pure mapping from the normalized store (Discord-shaped, snake_case, src/store/types.ts) to the Lounge
 * view model (camelCase, src/lounge/data.ts). No React here, so it is unit-testable on its own.
 */
import type {
  Channel,
  ChannelType,
  DmChannel,
  Guild,
  GuildPermissions,
  LoungeUser,
  Member,
  Message,
  Presence,
  Role,
  Snowflake,
} from './data';
import { cmpId } from '../store/reducer';
import { canOpenSettingsFromBits } from '../settings/server/permissions';
import type * as S from '../store/types';

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
  MODERATE_MEMBERS: 1n << 40n,
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
    serverSettings: canOpenSettingsFromBits(base),
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
    canSend,
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

export function toLoungePresence(p: S.Presence | undefined): Presence {
  return { status: p?.status ?? 'offline' };
}

export function toLoungeMember(m: S.Member, user: LoungeUser): Member {
  return {
    user,
    guildId: m.guild_id,
    nick: m.nick ?? null,
    roleIds: m.roles,
  };
}

/** Converts one store message; reactions and reply target carry over, activity and embeds do not yet. */
export function toLoungeMessage(m: S.Message): Message {
  const reactions = m.reactions?.map((r) => ({
    emoji: r.emoji.name ?? r.emoji.id ?? '',
    count: r.count,
    me: r.me,
  }));
  return {
    id: m.id,
    channelId: m.channel_id,
    author: toLoungeUser(m.author),
    content: m.content,
    createdAt: m.timestamp,
    editedAt: m.edited_timestamp ?? null,
    replyToId: m.message_reference?.message_id ?? null,
    ...(reactions && reactions.length ? { reactions } : {}),
    pending: m.local?.status === 'sending',
    failed: m.local?.status === 'failed',
  };
}
