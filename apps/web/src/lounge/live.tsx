/**
 * Store-backed implementations of the Lounge data hooks (lounge/data.ts). Every hook here reads the
 * normalized store (src/store) and maps it to the view model. data.ts calls these unconditionally and
 * prefers the mock provider's values when one is present, so these only run in the live app.
 */
import { type ReactNode, useEffect, useMemo } from 'react';
import { DM_GUILD, connectGateway, disconnectGateway, rest, useStore } from '../store';
import { memberKey } from '../store/reducer';
import type * as S from '../store/types';
import type {
  Channel,
  DmChannel,
  Guild,
  LoungeActions,
  LoungeUser,
  Member,
  Message,
  MessagesState,
  Presence,
  RailEntry,
  Role,
  SendExtras,
  Snowflake,
} from './data';
import {
  basePermissions,
  isUnread,
  PERM,
  toDmChannel,
  toGuildPermissions,
  toLoungeChannel,
  toLoungeGuild,
  toLoungeMember,
  toLoungeMessage,
  toLoungePresence,
  toLoungeRole,
  toLoungeUser,
} from './liveMap';

const noop = () => {};
const ANON_USER: LoungeUser = { id: '', username: '', displayName: '' };
const EMPTY_MESSAGES: MessagesState = {
  messages: [],
  hasMore: false,
  loadingOlder: false,
  loading: false,
};

/** Lounge message objects are cached per store object so memoized rows skip unchanged messages. */
const messageCache = new WeakMap<S.Message, Message>();
const cachedMessage = (m: S.Message): Message => {
  let out = messageCache.get(m);
  if (!out) {
    out = toLoungeMessage(m);
    messageCache.set(m, out);
  }
  return out;
};

/** Everything the shell reads about guilds, channels, people and presence, derived in one pass. */
export interface LiveSnapshot {
  me: LoungeUser;
  users: Record<Snowflake, LoungeUser>;
  guilds: Guild[];
  rail: RailEntry[];
  channelsByGuild: Record<Snowflake, Channel[]>;
  dms: DmChannel[];
  rolesByGuild: Record<Snowflake, Role[]>;
  membersByGuild: Record<Snowflake, Member[]>;
  presence: Record<Snowflake, Presence>;
}

export interface SnapshotInput {
  currentUserId: string | null;
  users: Record<string, S.User>;
  guilds: Record<string, S.Guild>;
  guildOrder: string[];
  channels: Record<string, S.Channel>;
  channelIdsByGuild: Record<string, string[]>;
  roles: Record<string, S.Role>;
  roleIdsByGuild: Record<string, string[]>;
  members: Record<string, S.Member>;
  memberIdsByGuild: Record<string, string[]>;
  presences: Record<string, S.Presence>;
  readStates: Record<string, S.ReadState>;
}

const byPosition = (a: S.Channel, b: S.Channel) => (a.position ?? 0) - (b.position ?? 0);

/** Pure: builds the snapshot from raw store slices. */
export function buildLiveSnapshot(s: SnapshotInput): LiveSnapshot {
  const users: Record<Snowflake, LoungeUser> = {};
  for (const u of Object.values(s.users)) users[u.id] = toLoungeUser(u);
  const me = s.currentUserId && users[s.currentUserId] ? users[s.currentUserId]! : ANON_USER;

  const presence: Record<Snowflake, Presence> = {};
  for (const [id, p] of Object.entries(s.presences)) presence[id] = toLoungePresence(p);

  const channelsByGuild: Record<Snowflake, Channel[]> = {};
  const rolesByGuild: Record<Snowflake, Role[]> = {};
  const membersByGuild: Record<Snowflake, Member[]> = {};
  const guilds: Guild[] = [];

  for (const gid of s.guildOrder) {
    const g = s.guilds[gid];
    if (!g) continue;
    const roleObjs = (s.roleIdsByGuild[gid] ?? [])
      .map((id) => s.roles[id])
      .filter((r): r is S.Role => !!r);
    const roleById = new Map(roleObjs.map((r) => [r.id, r]));
    rolesByGuild[gid] = roleObjs.map(toLoungeRole).sort((a, b) => b.position - a.position);

    const viewer = s.currentUserId ? s.members[memberKey(gid, s.currentUserId)] : undefined;
    const base = basePermissions(gid, g.owner_id, s.currentUserId ?? '', roleById, viewer?.roles);
    // Until the viewer's member object is loaded, the server decides; the composer stays enabled.
    const loaded = !!viewer || (!!g.owner_id && g.owner_id === s.currentUserId);
    const canSend = loaded ? (base & PERM.SEND_MESSAGES) === PERM.SEND_MESSAGES : true;

    const mapped = (s.channelIdsByGuild[gid] ?? [])
      .map((id) => s.channels[id])
      .filter((c): c is S.Channel => !!c)
      .sort(byPosition)
      .map((c) => toLoungeChannel(c, gid, s.readStates[c.id], canSend));
    channelsByGuild[gid] = mapped;

    const unread = mapped.some((c) => c.unread && !c.muted);
    const mentionCount = mapped.reduce((n, c) => n + c.mentionCount, 0);
    guilds.push(toLoungeGuild(g, toGuildPermissions(base), unread, mentionCount));

    const memberList: Member[] = [];
    for (const uid of s.memberIdsByGuild[gid] ?? []) {
      const m = s.members[memberKey(gid, uid)];
      const u = users[uid];
      if (m && u) memberList.push(toLoungeMember(m, u));
    }
    membersByGuild[gid] = memberList;
  }

  // Direct messages live under the '@me' pseudo guild, newest activity first.
  const dms: DmChannel[] = [];
  for (const id of s.channelIdsByGuild[DM_GUILD] ?? []) {
    const c = s.channels[id];
    if (c && (c.type === 1 || c.type === 3)) dms.push(toDmChannel(c, s.readStates[c.id]));
  }
  dms.sort((a, b) =>
    a.lastMessageAt < b.lastMessageAt ? 1 : a.lastMessageAt > b.lastMessageAt ? -1 : 0,
  );

  return {
    me,
    users,
    guilds,
    rail: s.guildOrder
      .filter((gid) => s.guilds[gid])
      .map((guildId): RailEntry => ({ kind: 'guild', guildId })),
    channelsByGuild,
    dms,
    rolesByGuild,
    membersByGuild,
    presence,
  };
}

/** Subscribes to the slices the snapshot reads and rebuilds it only when one of them changes. */
export function useLiveSnapshot(): LiveSnapshot {
  const currentUserId = useStore((s) => s.currentUserId);
  const users = useStore((s) => s.users);
  const guilds = useStore((s) => s.guilds);
  const guildOrder = useStore((s) => s.guildOrder);
  const channels = useStore((s) => s.channels);
  const channelIdsByGuild = useStore((s) => s.channelIdsByGuild);
  const roles = useStore((s) => s.roles);
  const roleIdsByGuild = useStore((s) => s.roleIdsByGuild);
  const members = useStore((s) => s.members);
  const memberIdsByGuild = useStore((s) => s.memberIdsByGuild);
  const presences = useStore((s) => s.presences);
  const readStates = useStore((s) => s.readStates);
  return useMemo(
    () =>
      buildLiveSnapshot({
        currentUserId,
        users,
        guilds,
        guildOrder,
        channels,
        channelIdsByGuild,
        roles,
        roleIdsByGuild,
        members,
        memberIdsByGuild,
        presences,
        readStates,
      }),
    [
      currentUserId,
      users,
      guilds,
      guildOrder,
      channels,
      channelIdsByGuild,
      roles,
      roleIdsByGuild,
      members,
      memberIdsByGuild,
      presences,
      readStates,
    ],
  );
}

/** Loads a guild's channels and roles over REST when only the READY stub is known. */
export function useEnsureGuild(guildId: string | undefined, active: boolean) {
  const loaded = useStore((s) => (guildId ? s.channelIdsByGuild[guildId] !== undefined : true));
  useEffect(() => {
    if (!active || !guildId || guildId === DM_GUILD || loaded) return;
    useStore.getState().loadGuild(guildId).catch(noop);
  }, [active, guildId, loaded]);
}

/** Loads a guild's member list once per guild. */
export function useEnsureMembers(guildId: string | undefined, active: boolean) {
  const loaded = useStore((s) => (guildId ? s.memberIdsByGuild[guildId] !== undefined : true));
  useEffect(() => {
    if (!active || !guildId || guildId === DM_GUILD || loaded) return;
    useStore.getState().loadMembers(guildId).catch(noop);
  }, [active, guildId, loaded]);
}

export function useLiveMessages(channelId: string | undefined, active: boolean): MessagesState {
  const win = useStore((s) => (channelId ? s.windows[channelId] : undefined));
  const byId = useStore((s) => s.messages);
  // READY (fresh page load or reconnect) drops cached history, which removes the window: load again.
  const hasWindow = win !== undefined;
  useEffect(() => {
    if (active && channelId) void useStore.getState().loadInitialMessages(channelId);
  }, [active, channelId, hasWindow]);
  return useMemo(() => {
    if (!win) return EMPTY_MESSAGES;
    const messages = [...win.ids, ...win.pending]
      .map((id) => byId[id])
      .filter((m): m is S.Message => m !== undefined)
      .map(cachedMessage);
    return {
      messages,
      hasMore: win.hasMoreBefore,
      loadingOlder: win.loading && win.ids.length > 0,
      loading: win.loading && win.ids.length === 0,
      atLatest: win.atLatest,
    };
  }, [win, byId]);
}

/** Users typing in a channel now (not the viewer). Expired entries are pruned by a timer. */
export function useLiveTypingIds(channelId: string | undefined): Snowflake[] {
  const typing = useStore((s) => (channelId ? s.typing[channelId] : undefined));
  const users = useStore((s) => s.users);
  const pruneTyping = useStore((s) => s.pruneTyping);
  useEffect(() => {
    if (!typing?.length) return;
    const next = Math.min(...typing.map((t) => t.expires_at));
    const timer = setTimeout(() => pruneTyping(), Math.max(0, next - Date.now()) + 5);
    return () => clearTimeout(timer);
  }, [typing, pruneTyping]);
  return useMemo(() => {
    const now = Date.now();
    return (typing ?? [])
      .filter((t) => t.expires_at > now && users[t.user_id])
      .map((t) => t.user_id);
  }, [typing, users]);
}

export function useLivePresence(userId: Snowflake): Presence {
  const p = useStore((s) => (userId ? s.presences[userId] : undefined));
  return useMemo(() => toLoungePresence(p), [p]);
}

/** View-model send extras to the wire shape. */
export function toPostExtras(x: SendExtras): rest.PostExtras {
  return {
    ...(x.attachments?.length
      ? {
          attachments: x.attachments.map((a) => ({
            id: a.id,
            uploaded_filename: a.uploadedFilename,
            ...(a.description ? { description: a.description } : {}),
            ...(a.durationSecs ? { duration_secs: a.durationSecs } : {}),
            ...(a.waveform ? { waveform: a.waveform } : {}),
          })),
        }
      : {}),
    ...(x.poll
      ? {
          poll: {
            question: { text: x.poll.question },
            answers: x.poll.answers.map((a) => ({
              poll_media: { text: a.text, ...(a.emoji ? { emoji: { name: a.emoji } } : {}) },
            })),
            duration: x.poll.durationHours,
            allow_multiselect: x.poll.multi,
          },
        }
      : {}),
    ...(x.mentionRepliedUser === false ? { allow_reply_ping: false } : {}),
    ...(x.flags ? { flags: x.flags } : {}),
  };
}

/** Actions are plain functions over the store, so one object serves the whole app. */
export const LIVE_ACTIONS: LoungeActions = {
  sendMessage(channelId, content, replyToId, extras) {
    useStore
      .getState()
      .sendMessage(channelId, content, replyToId ?? null, extras ? toPostExtras(extras) : undefined)
      .catch(noop);
  },
  deleteMessage(channelId, messageId) {
    useStore.getState().deleteMessage(channelId, messageId).catch(noop);
  },
  pinMessage(channelId, messageId, pinned) {
    useStore.getState().pinMessage(channelId, messageId, pinned).catch(noop);
  },
  forwardMessage(source, targets, note) {
    const st = useStore.getState();
    for (const target of targets) {
      st.forwardMessage(target, source)
        .then(() => {
          if (note) st.sendMessage(target, note, null).catch(noop);
        })
        .catch(noop);
    }
  },
  votePoll(channelId, messageId, answerId, add) {
    useStore.getState().votePoll(channelId, messageId, answerId, add).catch(noop);
  },
  endPoll(channelId, messageId) {
    useStore.getState().endPoll(channelId, messageId).catch(noop);
  },
  retryMessage(channelId, messageId) {
    const nonce = useStore.getState().messages[messageId]?.local?.nonce;
    if (nonce) useStore.getState().retryMessage(channelId, nonce).catch(noop);
  },
  discardMessage(channelId, messageId) {
    const nonce = useStore.getState().messages[messageId]?.local?.nonce;
    if (nonce) useStore.getState().discardMessage(channelId, nonce);
  },
  editMessage(channelId, messageId, content) {
    useStore.getState().editMessage(channelId, messageId, content).catch(noop);
  },
  loadOlder(channelId) {
    void useStore.getState().loadOlderMessages(channelId);
  },
  markRead(channelId) {
    useStore.getState().ackChannel(channelId);
  },
  markGuildRead(guildId) {
    const s = useStore.getState();
    for (const id of s.channelIdsByGuild[guildId] ?? []) {
      const c = s.channels[id];
      if (c && c.type !== 4 && isUnread(c, s.readStates[id])) s.ackChannel(id);
    }
  },
  toggleReaction(channelId, messageId, emoji) {
    void useStore.getState().toggleReaction(channelId, messageId, emoji);
  },
  startTyping(channelId) {
    rest.postTyping(channelId).catch(noop);
  },
  revealMessage(channelId, messageId) {
    void useStore.getState().loadAroundMessage(channelId, messageId);
  },
  jumpToPresent(channelId) {
    void useStore.getState().jumpToPresent(channelId);
  },
};

/** Opens the gateway for as long as the Lounge is mounted (one connection per page). */
export function LiveDataConnection({ children }: { children: ReactNode }) {
  useEffect(() => {
    connectGateway();
    return () => disconnectGateway();
  }, []);
  return <>{children}</>;
}
