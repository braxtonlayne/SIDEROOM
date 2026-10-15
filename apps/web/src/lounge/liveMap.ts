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
import { cdnUrl } from '../store/cdn';
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
  CONNECT: 1n << 20n,
  SPEAK: 1n << 21n,
  MUTE_MEMBERS: 1n << 22n,
  DEAFEN_MEMBERS: 1n << 23n,
  MOVE_MEMBERS: 1n << 24n,
  USE_VAD: 1n << 25n,
  ADMINISTRATOR: 1n << 3n,
  MANAGE_CHANNELS: 1n << 4n,
  MANAGE_GUILD: 1n << 5n,
  SEND_MESSAGES: 1n << 11n,
  VIEW_CHANNEL: 1n << 10n,
  MANAGE_MESSAGES: 1n << 13n,
  MENTION_EVERYONE: 1n << 17n,
  MODERATE_MEMBERS: 1n << 40n,
  PIN_MESSAGES: 1n << 51n,
  SEND_TTS_MESSAGES: 1n << 12n,
  CREATE_PUBLIC_THREADS: 1n << 35n,
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
    sendTts: has(PERM.SEND_TTS_MESSAGES),
    createThreads: has(PERM.CREATE_PUBLIC_THREADS),
    connect: has(PERM.CONNECT),
    speak: has(PERM.SPEAK),
    useVad: has(PERM.USE_VAD),
    muteMembers: has(PERM.MUTE_MEMBERS),
    deafenMembers: has(PERM.DEAFEN_MEMBERS),
    moveMembers: has(PERM.MOVE_MEMBERS),
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
    ...(c.nsfw ? { nsfw: true } : {}),
    ...(permissions ? { permissions } : {}),
    ...(c.rate_limit_per_user ? { slowmodeSeconds: c.rate_limit_per_user } : {}),
    ...(c.user_limit ? { userLimit: c.user_limit } : {}),
  };
}

export function toDmChannel(c: S.Channel, rs: S.ReadState | undefined): DmChannel {
  return {
    id: c.id,
    recipientIds: (c.recipients ?? []).map((u) => u.id),
    name: c.name ?? null,
    ...(c.owner_id ? { ownerId: c.owner_id } : {}),
    ...(c.icon ? { iconUrl: cdnUrl('channel-icons', c.id, c.icon, { size: 128 }) } : {}),
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
    ...(g.is_staff_guild ? { isStaff: true } : {}),
    unread,
    mentionCount,
    permissions: perms,
  };
}

const hex = (n: number) => `#${n.toString(16).padStart(6, '0')}`;

export function toLoungeRole(r: S.Role): Role {
  const secondary = r.colors?.secondary_color;
  return {
    id: r.id,
    name: r.name,
    color: r.color ? hex(r.color) : null,
    hoist: !!r.hoist,
    position: r.position,
    ...(secondary ? { colorSecondary: hex(secondary) } : {}),
    ...(r.unicode_emoji ? { emoji: r.unicode_emoji } : {}),
  };
}

const ACTIVITY_KIND: Record<number, Activity['kind']> = {
  0: 'playing',
  2: 'listening',
  3: 'watching',
  4: 'custom',
  /** SIDE ROOM's "Reading" (Discord has no type 6; contract with the presence backend). */
  6: 'reading',
};

interface WireActivity {
  type?: number;
  name?: string;
  state?: string | null;
  details?: string | null;
  timestamps?: { start?: number | null } | null;
  /** Game platform (Discord's `platform` field). */
  platform?: string | null;
  /** SIDE ROOM watch party that can be joined. */
  party?: { id?: string | null } | null;
  expires_at_ms?: number | null;
}

function toActivity(a: WireActivity): Activity | null {
  if (a.type === undefined) return null;
  const kind = ACTIVITY_KIND[a.type];
  if (!kind) return null;
  // Watching/reading carry the application in `name` ("SIDE ROOM Cinema") and the title in `details`.
  const titled = (a.type === 3 || a.type === 6) && !!a.details;
  const text = (a.type === 4 ? (a.state ?? a.name) : titled ? a.details : a.name) ?? '';
  if (!text) return null;
  const out: Activity = { kind, text };
  const sub = titled ? a.state : (a.details ?? a.state);
  if (a.type !== 4 && sub) out.details = sub;
  if (a.timestamps?.start) out.startedAt = a.timestamps.start;
  if (a.platform) out.platform = a.platform;
  if (a.party?.id) out.partyId = a.party.id;
  if (a.expires_at_ms) out.expiresAt = a.expires_at_ms;
  return out;
}

export function toLoungePresence(p: S.Presence | undefined, now = Date.now()): Presence {
  const status = p?.status ?? 'offline';
  const all = ((p?.activities as WireActivity[] | undefined) ?? [])
    .map(toActivity)
    .filter((a): a is Activity => a !== null && !(a.expiresAt && a.expiresAt <= now));
  const custom = all.find((a) => a.kind === 'custom');
  const rich = all.filter((a) => a.kind !== 'custom');
  const first = all[0];
  if (!first) return { status };
  return {
    status,
    activity: first,
    ...(rich.length ? { activities: rich } : {}),
    ...(custom ? { custom } : {}),
  };
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
const FLAG_EPHEMERAL = 1 << 6;

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
    ...(a.blurhash ? { blurhash: a.blurhash } : {}),
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
            ...(m.message_reference?.channel_id
              ? {
                  source: {
                    channelId: m.message_reference.channel_id,
                    messageId: m.message_reference.message_id,
                    ...(m.message_reference.guild_id
                      ? { guildId: m.message_reference.guild_id }
                      : {}),
                  },
                }
              : {}),
            content: snap.content ?? '',
            createdAt: snap.timestamp ?? m.timestamp,
            attachments: (snap.attachments ?? []).map((a) => toLoungeAttachment(a)),
            embeds: snap.embeds ?? [],
          },
        }
      : {}),
    ...(m.message_reference?.channel_id && m.type
      ? { referenceChannelId: m.message_reference.channel_id }
      : {}),
    ...(m.tts ? { tts: true } : {}),
    ...(m.thread
      ? {
          thread: {
            id: m.thread.id,
            name: m.thread.name ?? 'Thread',
            messageCount: m.thread.message_count ?? 0,
          },
        }
      : {}),
    ...(flags & FLAG_VOICE ? { voice: true } : {}),
    ...(flags & FLAG_EPHEMERAL ? { ephemeral: true } : {}),
    ...(flags & FLAG_SUPPRESS_EMBEDS ? { suppressEmbeds: true } : {}),
  };
}
