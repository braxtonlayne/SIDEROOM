// FND-API-008: pure reducer from gateway dispatches to store data. No I/O, fully unit-testable.
import type {
  Channel,
  Guild,
  Member,
  Message,
  MessageWindow,
  Presence,
  PresenceStatus,
  ReadState,
  Role,
  TypingEntry,
  User,
  UserSettingsBlob,
  VoiceServerInfo,
  VoiceState,
} from './types';

/** Key used in `channelIdsByGuild` for DM / group DM channels. */
export const DM_GUILD = '@me';
export const TYPING_TTL_MS = 10_000;
export const MAX_WINDOW = 300;
export const pendingId = (nonce: string) => `pending:${nonce}`;
export const memberKey = (guildId: string, userId: string) => `${guildId}:${userId}`;

export interface StoreData {
  currentUserId: string | null;
  users: Record<string, User>;
  guilds: Record<string, Guild>;
  guildOrder: string[];
  channels: Record<string, Channel>;
  channelIdsByGuild: Record<string, string[]>;
  roles: Record<string, Role>;
  roleIdsByGuild: Record<string, string[]>;
  members: Record<string, Member>;
  memberIdsByGuild: Record<string, string[]>;
  messages: Record<string, Message>;
  windows: Record<string, MessageWindow>;
  readStates: Record<string, ReadState>;
  presences: Record<string, Presence>;
  typing: Record<string, TypingEntry[]>;
  /** The synced per-user settings blob (D-179); null until loaded or pushed over the gateway. */
  userSettings: UserSettingsBlob | null;
  /** Highlight words (`/users/@me/notification-keywords`); null until fetched. */
  notificationKeywords: string[] | null;
  /**
   * Message-request DMs (`is_message_request`) from READY and CHANNEL_CREATE: kept out of `channels` until
   * accepted; the social store seeds the Message Requests page from here.
   */
  messageRequestDms: Channel[];
  /** Who is connected to voice: guild id (`@me` for DM calls) -> user id -> state. */
  voiceStates: Record<string, Record<string, VoiceState>>;
  /** The latest `VOICE_SERVER_UPDATE` for the viewer's pending / live connection. */
  voiceServer: VoiceServerInfo | null;
}

export const emptyData = (): StoreData => ({
  currentUserId: null,
  users: {},
  guilds: {},
  guildOrder: [],
  channels: {},
  channelIdsByGuild: {},
  roles: {},
  roleIdsByGuild: {},
  members: {},
  memberIdsByGuild: {},
  messages: {},
  windows: {},
  readStates: {},
  presences: {},
  typing: {},
  userSettings: null,
  notificationKeywords: null,
  messageRequestDms: [],
  voiceStates: {},
  voiceServer: null,
});

export const emptyWindow = (): MessageWindow => ({
  ids: [],
  pending: [],
  hasMoreBefore: false,
  atLatest: true,
  loading: false,
  error: null,
});

/** Snowflake order without BigInt: longer string is larger, then lexicographic. */
export function cmpId(a: string, b: string): number {
  if (a.length !== b.length) return a.length - b.length;
  return a < b ? -1 : a > b ? 1 : 0;
}

function insertSorted(ids: string[], id: string): string[] {
  if (ids.includes(id)) return ids;
  const last = ids[ids.length - 1];
  if (last === undefined || cmpId(id, last) > 0) return [...ids, id];
  const out = [...ids, id];
  out.sort(cmpId);
  return out;
}

/** Union of two id lists in snowflake order. */
export function mergeIds(a: string[], b: string[]): string[] {
  return [...new Set([...a, ...b])].sort(cmpId);
}

type Obj = Record<string, unknown>;
const str = (v: unknown): string => (typeof v === 'string' ? v : v == null ? '' : String(v));

function addUser(users: Record<string, User>, u: unknown): Record<string, User> {
  if (!u || typeof (u as User).id !== 'string') return users;
  const user = u as User;
  return { ...users, [user.id]: { ...users[user.id], ...user } };
}

/** Merge many users with one copy of the map (a copy per user made a big READY quadratic, K-22). */
function addUsers(users: Record<string, User>, list: unknown[]): Record<string, User> {
  let out: Record<string, User> | null = null;
  for (const u of list) {
    if (!u || typeof (u as User).id !== 'string') continue;
    const user = u as User;
    out ??= { ...users };
    out[user.id] = { ...out[user.id], ...user };
  }
  return out ?? users;
}

function toMember(guildId: string, m: Obj): Member | null {
  const user = m.user as User | undefined;
  const userId = user?.id ?? (m.user_id as string | undefined);
  if (!userId) return null;
  return {
    guild_id: guildId,
    user_id: userId,
    nick: (m.nick as string | null | undefined) ?? null,
    roles: (m.roles as string[] | undefined) ?? [],
    joined_at: m.joined_at as string | undefined,
    communication_disabled_until:
      (m.communication_disabled_until as string | null | undefined) ?? null,
  };
}

function putChannel(s: StoreData, c: Channel): Partial<StoreData> {
  const bucket = c.guild_id ?? DM_GUILD;
  const ids = s.channelIdsByGuild[bucket] ?? [];
  return {
    channels: { ...s.channels, [c.id]: { ...s.channels[c.id], ...c } },
    channelIdsByGuild: ids.includes(c.id)
      ? s.channelIdsByGuild
      : { ...s.channelIdsByGuild, [bucket]: [...ids, c.id] },
    users: c.recipients ? addUsers(s.users, c.recipients) : s.users,
  };
}

function putMembers(s: StoreData, guildId: string, list: Obj[]): Partial<StoreData> {
  const users = addUsers(
    s.users,
    list.map((raw) => raw.user),
  );
  const members = { ...s.members };
  const ids = new Set(s.memberIdsByGuild[guildId] ?? []);
  for (const raw of list) {
    const m = toMember(guildId, raw);
    if (!m) continue;
    members[memberKey(guildId, m.user_id)] = { ...members[memberKey(guildId, m.user_id)], ...m };
    ids.add(m.user_id);
  }
  return { users, members, memberIdsByGuild: { ...s.memberIdsByGuild, [guildId]: [...ids] } };
}

/** Add many channels of one bucket with one copy of the maps. */
function putChannels(s: StoreData, bucket: string, list: Channel[]): Partial<StoreData> {
  const channels = { ...s.channels };
  const ids = [...(s.channelIdsByGuild[bucket] ?? [])];
  const have = new Set(ids);
  const recipients: unknown[] = [];
  for (const c of list) {
    channels[c.id] = { ...channels[c.id], ...c };
    if (!have.has(c.id)) {
      have.add(c.id);
      ids.push(c.id);
    }
    if (c.recipients) recipients.push(...c.recipients);
  }
  return {
    channels,
    channelIdsByGuild: { ...s.channelIdsByGuild, [bucket]: ids },
    users: recipients.length ? addUsers(s.users, recipients) : s.users,
  };
}

function putPresences(s: StoreData, list: Obj[]): Record<string, Presence> {
  const out = { ...s.presences };
  for (const p of list) {
    const userId = (p.user as User | undefined)?.id ?? (p.user_id as string | undefined);
    if (!userId) continue;
    out[userId] = {
      user_id: userId,
      status: ((p.status as PresenceStatus | undefined) ?? 'offline') as PresenceStatus,
      activities: (p.activities as unknown[] | undefined) ?? [],
    };
  }
  return out;
}

export function removeFromWindowState(s: StoreData, channelId: string, messageId: string) {
  return removeFromWindow(s, channelId, [messageId]);
}

function removeFromWindow(
  s: StoreData,
  channelId: string,
  messageIds: string[],
): Partial<StoreData> {
  const w = s.windows[channelId];
  const messages = { ...s.messages };
  for (const id of messageIds) delete messages[id];
  if (!w) return { messages };
  const drop = new Set(messageIds);
  return {
    messages,
    windows: { ...s.windows, [channelId]: { ...w, ids: w.ids.filter((i) => !drop.has(i)) } },
  };
}

/** Insert a server message into its channel window, reconciling an optimistic twin by nonce. */
export function putServerMessage(s: StoreData, msg: Message): Partial<StoreData> {
  const users = addUser(s.users, msg.author);
  const w = s.windows[msg.channel_id];
  const messages = { ...s.messages };
  let window = w;
  if (w) {
    let pending = w.pending;
    const nonce = msg.nonce != null ? String(msg.nonce) : null;
    if (nonce) {
      const pid = pendingId(nonce);
      if (pending.includes(pid) && msg.author.id === s.currentUserId) {
        pending = pending.filter((p) => p !== pid);
        delete messages[pid];
      }
    }
    // Only append live messages when the window reaches the newest message.
    let ids = w.ids;
    let hasMoreBefore = w.hasMoreBefore;
    if (w.atLatest || ids.includes(msg.id)) {
      ids = insertSorted(ids, msg.id);
      if (ids.length > MAX_WINDOW) {
        for (const old of ids.slice(0, ids.length - MAX_WINDOW)) delete messages[old];
        ids = ids.slice(ids.length - MAX_WINDOW);
        hasMoreBefore = true;
      }
    }
    window = { ...w, ids, pending, hasMoreBefore };
  }
  messages[msg.id] = { ...messages[msg.id], ...msg };
  const ch = s.channels[msg.channel_id];
  const bumpChannel =
    ch && (!ch.last_message_id || cmpId(msg.id, ch.last_message_id) > 0)
      ? { channels: { ...s.channels, [ch.id]: { ...ch, last_message_id: msg.id } } }
      : {};
  return {
    users,
    messages,
    ...(window ? { windows: { ...s.windows, [msg.channel_id]: window } } : {}),
    ...bumpChannel,
  };
}

/**
 * The part of a GUILD_CREATE the first paint needs: the server, its channels, roles and voice states. The
 * members and presences are indexed by [`reduceGuildPeople`], which a big READY can run a moment later
 * (K-22).
 */
export function reduceGuildCreate(s: StoreData, d: Obj): StoreData {
  const id = str(d.id);
  const {
    channels,
    threads: _t,
    roles,
    members: _m,
    presences: _p,
    ...rest
  } = d as Obj & {
    channels?: Channel[];
    roles?: Role[];
    members?: Obj[];
    presences?: Obj[];
    threads?: unknown;
  };
  void _t;
  void _m;
  void _p;
  let next: StoreData = {
    ...s,
    guilds: {
      ...s.guilds,
      [id]: { ...s.guilds[id], ...(rest as object), id, unavailable: false } as Guild,
    },
    guildOrder: s.guildOrder.includes(id) ? s.guildOrder : [...s.guildOrder, id],
  };
  // Replace this guild's channel index with the authoritative list.
  const chMap = { ...next.channels };
  for (const oldId of next.channelIdsByGuild[id] ?? []) delete chMap[oldId];
  next = {
    ...next,
    channels: chMap,
    channelIdsByGuild: { ...next.channelIdsByGuild, [id]: [] },
  };
  next = {
    ...next,
    ...putChannels(
      next,
      id,
      (channels ?? []).map((c) => ({ ...c, guild_id: id })),
    ),
  };
  const roleMap = { ...next.roles };
  for (const oldId of next.roleIdsByGuild[id] ?? []) delete roleMap[oldId];
  for (const r of roles ?? []) roleMap[r.id] = { ...r, guild_id: id };
  next = {
    ...next,
    roles: roleMap,
    roleIdsByGuild: { ...next.roleIdsByGuild, [id]: (roles ?? []).map((r) => r.id) },
  };
  const voice = d.voice_states as VoiceState[] | undefined;
  if (voice) {
    const map: Record<string, VoiceState> = {};
    for (const v of voice) if (v.channel_id) map[v.user_id] = { ...v, guild_id: id };
    next = { ...next, voiceStates: { ...next.voiceStates, [id]: map } };
  }
  return next;
}

/** Index the members and presences that came with a GUILD_CREATE. */
export function reduceGuildPeople(
  s: StoreData,
  guildId: string,
  members?: Obj[],
  presences?: Obj[],
): Partial<StoreData> {
  let out: Partial<StoreData> = {};
  if (members) out = putMembers(s, guildId, members);
  if (presences) out = { ...out, presences: putPresences({ ...s, ...out }, presences) };
  return out;
}

/** Apply one gateway dispatch; returns the changed slices (empty object = ignored). */
export function reduceDispatch(
  s: StoreData,
  event: string,
  data: unknown,
  now = Date.now(),
): Partial<StoreData> {
  const d = (data ?? {}) as Obj;
  switch (event) {
    case 'READY': {
      const user = d.user as User;
      let next: StoreData = {
        ...s,
        currentUserId: user.id,
        users: addUser(s.users, user),
      };
      const stubs = (d.guilds as Obj[] | undefined) ?? [];
      const guilds = { ...s.guilds };
      for (const g of stubs) {
        const id = str(g.id);
        guilds[id] = {
          ...guilds[id],
          name: guilds[id]?.name ?? '',
          ...(g as object),
          id,
          unavailable: true,
        };
      }
      next = { ...next, guilds, guildOrder: stubs.map((g) => str(g.id)) };
      const requests: Channel[] = [];
      for (const c of (d.private_channels as Channel[] | undefined) ?? []) {
        if (c.is_message_request) requests.push(c);
        else next = { ...next, ...putChannel(next, c) };
      }
      next = { ...next, messageRequestDms: requests };
      const rs = d.read_state as { entries?: Obj[] } | Obj[] | undefined;
      const entries = Array.isArray(rs) ? rs : (rs?.entries ?? []);
      const readStates: Record<string, ReadState> = {};
      for (const e of entries) {
        const id = str(e.id ?? e.channel_id);
        readStates[id] = {
          channel_id: id,
          last_message_id: (e.last_message_id as string | null | undefined) ?? null,
          mention_count: Number(e.mention_count ?? 0),
        };
      }
      // A fresh session may have missed events. The cached history stays on screen (an empty channel until
      // the next fetch is worse) but is marked stale, so the newest page is fetched again and replaces it
      // (K-22). Optimistic sends are kept as they are.
      const windows: Record<string, MessageWindow> = {};
      const messages: Record<string, Message> = {};
      for (const [cid, w] of Object.entries(s.windows)) {
        if (!w.ids.length && !w.pending.length) continue;
        windows[cid] = w.ids.length
          ? { ...w, loading: false, error: null, stale: true }
          : { ...emptyWindow(), pending: w.pending };
        for (const id of [...w.ids, ...w.pending]) {
          const m = s.messages[id];
          if (m) messages[id] = m;
        }
      }
      return {
        currentUserId: next.currentUserId,
        users: next.users,
        guilds: next.guilds,
        guildOrder: next.guildOrder,
        channels: next.channels,
        channelIdsByGuild: next.channelIdsByGuild,
        messageRequestDms: requests,
        // GUILD_CREATE re-delivers every guild's voice states.
        voiceStates: {},
        readStates,
        presences: putPresences(next, (d.presences as Obj[] | undefined) ?? []),
        windows,
        messages,
      };
    }
    case 'GUILD_CREATE': {
      const core = reduceGuildCreate(s, d);
      const { members, presences } = d as { members?: Obj[]; presences?: Obj[] };
      if (!members && !presences) return core;
      return { ...core, ...reduceGuildPeople({ ...s, ...core }, str(d.id), members, presences) };
    }
    case 'GUILD_UPDATE': {
      const id = str(d.id);
      return { guilds: { ...s.guilds, [id]: { ...s.guilds[id], ...(d as object), id } as Guild } };
    }
    case 'GUILD_DELETE': {
      const id = str(d.id);
      if (d.unavailable === true) {
        const g = s.guilds[id];
        return g ? { guilds: { ...s.guilds, [id]: { ...g, unavailable: true } } } : {};
      }
      const guilds = { ...s.guilds };
      delete guilds[id];
      const channels = { ...s.channels };
      const windows = { ...s.windows };
      for (const cid of s.channelIdsByGuild[id] ?? []) {
        delete channels[cid];
        delete windows[cid];
      }
      const roles = { ...s.roles };
      for (const rid of s.roleIdsByGuild[id] ?? []) delete roles[rid];
      const members = { ...s.members };
      for (const uid of s.memberIdsByGuild[id] ?? []) delete members[memberKey(id, uid)];
      const without = <T>(r: Record<string, T>) => {
        const o = { ...r };
        delete o[id];
        return o;
      };
      return {
        guilds,
        guildOrder: s.guildOrder.filter((g) => g !== id),
        channels,
        windows,
        roles,
        members,
        channelIdsByGuild: without(s.channelIdsByGuild),
        roleIdsByGuild: without(s.roleIdsByGuild),
        memberIdsByGuild: without(s.memberIdsByGuild),
        voiceStates: without(s.voiceStates),
      };
    }
    case 'VOICE_STATE_UPDATE': {
      const userId = str(d.user_id);
      if (!userId) return {};
      const guildId = typeof d.guild_id === 'string' && d.guild_id ? d.guild_id : DM_GUILD;
      const next = { ...s.voiceStates[guildId] };
      if (d.channel_id == null) delete next[userId];
      else next[userId] = { ...(d as unknown as VoiceState), guild_id: guildId };
      const states = { ...s.voiceStates, [guildId]: next };
      // A user is in one voice channel at a time: drop a stale entry held under another guild.
      if (d.channel_id != null) {
        for (const [g, m] of Object.entries(states)) {
          if (g !== guildId && m[userId]) {
            const copy = { ...m };
            delete copy[userId];
            states[g] = copy;
          }
        }
      }
      return { voiceStates: states };
    }
    case 'VOICE_SERVER_UPDATE':
      return {
        voiceServer: {
          guild_id: typeof d.guild_id === 'string' ? d.guild_id : null,
          ...(typeof d.channel_id === 'string' ? { channel_id: d.channel_id } : {}),
          endpoint: str(d.endpoint),
          token: str(d.token),
          ...(Array.isArray(d.ice_servers) ? { ice_servers: d.ice_servers } : {}),
        } as VoiceServerInfo,
      };
    case 'CHANNEL_CREATE':
    case 'CHANNEL_UPDATE': {
      const c = d as unknown as Channel;
      // Message-request DMs live in the social store until accepted; an accepted one arrives again.
      if (!c.guild_id && c.is_message_request) {
        const others = s.messageRequestDms.filter((r) => r.id !== c.id);
        return { messageRequestDms: [c, ...others] };
      }
      const accepted = s.messageRequestDms.some((r) => r.id === c.id);
      return {
        ...putChannel(s, c),
        ...(accepted
          ? { messageRequestDms: s.messageRequestDms.filter((r) => r.id !== c.id) }
          : {}),
      };
    }
    case 'CHANNEL_DELETE': {
      const id = str(d.id);
      const c = s.channels[id];
      const bucket = c?.guild_id ?? (d.guild_id as string | undefined) ?? DM_GUILD;
      const channels = { ...s.channels };
      delete channels[id];
      const windows = { ...s.windows };
      delete windows[id];
      const readStates = { ...s.readStates };
      delete readStates[id];
      return {
        channels,
        windows,
        readStates,
        channelIdsByGuild: {
          ...s.channelIdsByGuild,
          [bucket]: (s.channelIdsByGuild[bucket] ?? []).filter((x) => x !== id),
        },
        ...(s.messageRequestDms.some((r) => r.id === id)
          ? { messageRequestDms: s.messageRequestDms.filter((r) => r.id !== id) }
          : {}),
      };
    }
    case 'GUILD_ROLE_CREATE':
    case 'GUILD_ROLE_UPDATE': {
      const guildId = str(d.guild_id);
      const role = { ...(d.role as Role), guild_id: guildId };
      const ids = s.roleIdsByGuild[guildId] ?? [];
      return {
        roles: { ...s.roles, [role.id]: { ...s.roles[role.id], ...role } },
        roleIdsByGuild: ids.includes(role.id)
          ? s.roleIdsByGuild
          : { ...s.roleIdsByGuild, [guildId]: [...ids, role.id] },
      };
    }
    case 'GUILD_ROLE_DELETE': {
      const guildId = str(d.guild_id);
      const roleId = str(d.role_id);
      const roles = { ...s.roles };
      delete roles[roleId];
      const members = { ...s.members };
      for (const uid of s.memberIdsByGuild[guildId] ?? []) {
        const k = memberKey(guildId, uid);
        const m = members[k];
        if (m?.roles.includes(roleId))
          members[k] = { ...m, roles: m.roles.filter((r) => r !== roleId) };
      }
      return {
        roles,
        members,
        roleIdsByGuild: {
          ...s.roleIdsByGuild,
          [guildId]: (s.roleIdsByGuild[guildId] ?? []).filter((r) => r !== roleId),
        },
      };
    }
    case 'GUILD_MEMBER_ADD':
    case 'GUILD_MEMBER_UPDATE':
      return putMembers(s, str(d.guild_id), [d]);
    case 'GUILD_MEMBERS_CHUNK': {
      const guildId = str(d.guild_id);
      const out = putMembers(s, guildId, (d.members as Obj[] | undefined) ?? []);
      return d.presences
        ? { ...out, presences: putPresences({ ...s, ...out }, d.presences as Obj[]) }
        : out;
    }
    case 'GUILD_MEMBER_REMOVE': {
      const guildId = str(d.guild_id);
      const userId = (d.user as User).id;
      const members = { ...s.members };
      delete members[memberKey(guildId, userId)];
      return {
        members,
        memberIdsByGuild: {
          ...s.memberIdsByGuild,
          [guildId]: (s.memberIdsByGuild[guildId] ?? []).filter((u) => u !== userId),
        },
      };
    }
    case 'SR_USER_SETTINGS_UPDATE': {
      // Another session (or our own PATCH) saved the blob; older versions never overwrite newer ones.
      const version = Number(d.version ?? 0);
      const settings = d.settings as Record<string, unknown> | undefined;
      if (!settings || typeof settings !== 'object') return {};
      if (s.userSettings && s.userSettings.version > version) return {};
      return { userSettings: { version, settings } };
    }
    case 'SR_NOTIFICATION_KEYWORDS_UPDATE': {
      const list = Array.isArray(d.keywords)
        ? d.keywords.filter((k): k is string => typeof k === 'string')
        : [];
      return { notificationKeywords: list };
    }
    case 'USER_UPDATE':
      return { users: addUser(s.users, d) };
    case 'PRESENCE_UPDATE':
      return { presences: putPresences(s, [d]) };
    case 'SR_ACTIVITY_UPDATE': {
      // "Watching" presence for friends (docs/05 6.3): fold the activity into the person's presence, replacing any
      // earlier Watching one; `stopped` removes it. A missed `stopped` is covered by `expires_at_ms`.
      const userId = str(d.user_id);
      if (!userId) return {};
      const prev = s.presences[userId] ?? {
        user_id: userId,
        status: 'online' as PresenceStatus,
        activities: [],
      };
      const others = (prev.activities as Obj[]).filter((a) => a.type !== 3);
      const activity = d.type === 'watching' ? (d.activity as Obj | undefined) : undefined;
      const merged = activity
        ? [
            ...others,
            {
              ...activity,
              type: 3,
              ...(d.expires_at_ms ? { expires_at_ms: d.expires_at_ms } : {}),
            },
          ]
        : others;
      return { presences: { ...s.presences, [userId]: { ...prev, activities: merged } } };
    }
    case 'TYPING_START': {
      const channelId = str(d.channel_id);
      const userId = str(d.user_id);
      if (userId === s.currentUserId) return {};
      const rest = (s.typing[channelId] ?? []).filter(
        (t) => t.user_id !== userId && t.expires_at > now,
      );
      const member = d.member as Obj | undefined;
      const guildId = d.guild_id as string | undefined;
      const extra = member && guildId ? putMembers(s, guildId, [member]) : {};
      return {
        ...extra,
        typing: {
          ...s.typing,
          [channelId]: [...rest, { user_id: userId, expires_at: now + TYPING_TTL_MS }],
        },
      };
    }
    case 'MESSAGE_CREATE': {
      const msg = d as unknown as Message;
      const out = putServerMessage(s, msg);
      const typing = s.typing[msg.channel_id];
      const typingOut = typing?.some((t) => t.user_id === msg.author.id)
        ? {
            typing: {
              ...s.typing,
              [msg.channel_id]: typing.filter((t) => t.user_id !== msg.author.id),
            },
          }
        : {};
      const mentioned =
        s.currentUserId !== null &&
        msg.author.id !== s.currentUserId &&
        ((msg.mentions ?? []).some((u) => u.id === s.currentUserId) || msg.mention_everyone);
      let readOut = {};
      if (mentioned) {
        const rs = s.readStates[msg.channel_id] ?? {
          channel_id: msg.channel_id,
          last_message_id: null,
          mention_count: 0,
        };
        readOut = {
          readStates: {
            ...s.readStates,
            [msg.channel_id]: { ...rs, mention_count: rs.mention_count + 1 },
          },
        };
      }
      return { ...out, ...typingOut, ...readOut };
    }
    case 'MESSAGE_UPDATE': {
      const id = str(d.id);
      const existing = s.messages[id];
      if (!existing) return {};
      // A full message object (it has `content`) leaves out `components` when it has none: a card the
      // server rewrote without its buttons must lose them here too, not keep the old rows.
      const full = 'content' in d;
      return {
        messages: {
          ...s.messages,
          [id]: {
            ...existing,
            ...(full ? { components: undefined } : {}),
            ...(d as object),
          } as Message,
        },
        users: d.author ? addUser(s.users, d.author) : s.users,
      };
    }
    case 'MESSAGE_REACTION_ADD':
    case 'MESSAGE_REACTION_REMOVE':
    case 'MESSAGE_REACTION_REMOVE_ALL':
    case 'MESSAGE_REACTION_REMOVE_EMOJI': {
      const id = str(d.message_id);
      const existing = s.messages[id];
      if (!existing) return {};
      return {
        messages: { ...s.messages, [id]: applyReactionEvent(existing, event, d, s.currentUserId) },
      };
    }
    case 'MESSAGE_POLL_VOTE_ADD':
    case 'MESSAGE_POLL_VOTE_REMOVE': {
      const id = str(d.message_id);
      const existing = s.messages[id];
      const results = existing?.poll?.results;
      if (!existing?.poll || !results) return {};
      const add = event === 'MESSAGE_POLL_VOTE_ADD';
      const answerId = Number(d.answer_id);
      const mine = str(d.user_id) === s.currentUserId;
      const answer_counts = results.answer_counts.map((c) =>
        c.id === answerId
          ? {
              ...c,
              count: Math.max(0, c.count + (add ? 1 : -1)),
              me_voted: mine ? add : c.me_voted,
            }
          : c,
      );
      return {
        messages: {
          ...s.messages,
          [id]: { ...existing, poll: { ...existing.poll, results: { ...results, answer_counts } } },
        },
      };
    }
    case 'MESSAGE_DELETE':
      return removeFromWindow(s, str(d.channel_id), [str(d.id)]);
    case 'MESSAGE_DELETE_BULK':
      return removeFromWindow(s, str(d.channel_id), (d.ids as string[] | undefined) ?? []);
    case 'MESSAGE_ACK': {
      const channelId = str(d.channel_id);
      return {
        readStates: {
          ...s.readStates,
          [channelId]: {
            channel_id: channelId,
            last_message_id: (d.message_id as string | undefined) ?? null,
            mention_count: Number(d.mention_count ?? 0),
          },
        },
      };
    }
    default:
      return {};
  }
}

/** Applies a MESSAGE_REACTION_* dispatch to one message (the viewer's own events keep `me`). */
function applyReactionEvent(m: Message, event: string, d: Obj, selfId: string | null): Message {
  const e = (d.emoji ?? {}) as { id?: string | null; name?: string | null };
  const key = e.name ?? e.id ?? '';
  const list = [...(m.reactions ?? [])];
  const i = list.findIndex((r) => (r.emoji.name ?? r.emoji.id ?? '') === key);
  const cur = list[i];
  const mine = str(d.user_id) === selfId;
  switch (event) {
    case 'MESSAGE_REACTION_ADD':
      if (cur) {
        // The viewer's own optimistic toggle already counted this reaction.
        if (!(mine && cur.me)) list[i] = { ...cur, count: cur.count + 1, me: cur.me || mine };
      } else list.push({ emoji: { id: e.id ?? null, name: e.name ?? null }, count: 1, me: mine });
      break;
    case 'MESSAGE_REACTION_REMOVE':
      if (cur) {
        if (mine && !cur.me) break;
        if (cur.count <= 1) list.splice(i, 1);
        else list[i] = { ...cur, count: cur.count - 1, me: mine ? false : cur.me };
      }
      break;
    case 'MESSAGE_REACTION_REMOVE_EMOJI':
      if (cur) list.splice(i, 1);
      break;
    default:
      list.length = 0;
  }
  return { ...m, reactions: list };
}
