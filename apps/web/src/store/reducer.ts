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

function addUsers(users: Record<string, User>, list: unknown[]): Record<string, User> {
  let out = users;
  for (const u of list) out = addUser(out, u);
  return out;
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
  let users = s.users;
  const members = { ...s.members };
  const ids = new Set(s.memberIdsByGuild[guildId] ?? []);
  for (const raw of list) {
    const m = toMember(guildId, raw);
    if (!m) continue;
    users = addUser(users, raw.user);
    members[memberKey(guildId, m.user_id)] = { ...members[memberKey(guildId, m.user_id)], ...m };
    ids.add(m.user_id);
  }
  return { users, members, memberIdsByGuild: { ...s.memberIdsByGuild, [guildId]: [...ids] } };
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
      for (const c of (d.private_channels as Channel[] | undefined) ?? []) {
        next = { ...next, ...putChannel(next, c) };
      }
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
      // A fresh session may have missed events: drop cached history, keep optimistic sends.
      const windows: Record<string, MessageWindow> = {};
      const messages: Record<string, Message> = {};
      for (const [cid, w] of Object.entries(s.windows)) {
        if (!w.pending.length) continue;
        windows[cid] = { ...emptyWindow(), pending: w.pending };
        for (const p of w.pending) {
          const m = s.messages[p];
          if (m) messages[p] = m;
        }
      }
      return {
        currentUserId: next.currentUserId,
        users: next.users,
        guilds: next.guilds,
        guildOrder: next.guildOrder,
        channels: next.channels,
        channelIdsByGuild: next.channelIdsByGuild,
        readStates,
        presences: putPresences(next, (d.presences as Obj[] | undefined) ?? []),
        windows,
        messages,
      };
    }
    case 'GUILD_CREATE': {
      const id = str(d.id);
      const {
        channels,
        threads: _t,
        roles,
        members,
        presences,
        ...rest
      } = d as Obj & {
        channels?: Channel[];
        roles?: Role[];
        members?: Obj[];
        presences?: Obj[];
        threads?: unknown;
      };
      void _t;
      let next: StoreData = {
        ...s,
        guilds: {
          ...s.guilds,
          [id]: { ...s.guilds[id], ...(rest as object), id, unavailable: false } as Guild,
        },
        guildOrder: s.guildOrder.includes(id) ? s.guildOrder : [...s.guildOrder, id],
      };
      const chList = channels ?? [];
      // Replace this guild's channel index with the authoritative list.
      const chMap = { ...next.channels };
      for (const oldId of next.channelIdsByGuild[id] ?? []) delete chMap[oldId];
      next = {
        ...next,
        channels: chMap,
        channelIdsByGuild: { ...next.channelIdsByGuild, [id]: [] },
      };
      for (const c of chList) next = { ...next, ...putChannel(next, { ...c, guild_id: id }) };
      const roleMap = { ...next.roles };
      for (const oldId of next.roleIdsByGuild[id] ?? []) delete roleMap[oldId];
      for (const r of roles ?? []) roleMap[r.id] = { ...r, guild_id: id };
      next = {
        ...next,
        roles: roleMap,
        roleIdsByGuild: { ...next.roleIdsByGuild, [id]: (roles ?? []).map((r) => r.id) },
      };
      if (members) next = { ...next, ...putMembers(next, id, members) };
      if (presences) next = { ...next, presences: putPresences(next, presences) };
      return next;
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
      };
    }
    case 'CHANNEL_CREATE':
    case 'CHANNEL_UPDATE':
      return putChannel(s, d as unknown as Channel);
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
    case 'USER_UPDATE':
      return { users: addUser(s.users, d) };
    case 'PRESENCE_UPDATE':
      return { presences: putPresences(s, [d]) };
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
      return {
        messages: { ...s.messages, [id]: { ...existing, ...(d as object) } as Message },
        users: d.author ? addUser(s.users, d.author) : s.users,
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
