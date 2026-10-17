// FND-API-008: React hooks over the store. Each selects stable references and derives arrays in
// useMemo, so they never return a fresh object from a selector (which would loop in Zustand 5).
import { useCallback, useEffect, useMemo, useState } from 'react';
import { DM_GUILD, cmpId, memberKey } from './reducer';
import { useStore } from './store';
import type {
  Channel,
  ConnectionStatus,
  Guild,
  Member,
  Message,
  Presence,
  ReadState,
  Role,
  User,
} from './types';

export interface MessagesApi {
  /** Oldest to newest; optimistic messages (message.local set) are last. */
  messages: Message[];
  hasMoreBefore: boolean;
  loading: boolean;
  error: string | null;
  /** Load the newest page if the channel has no window yet. Called automatically on mount. */
  loadInitial: () => Promise<void>;
  /** Load the page of history before the oldest loaded message. */
  loadOlder: () => Promise<void>;
  /** Optimistically send; resolves with the server message, rejects if the REST call fails. */
  send: (content: string) => Promise<Message>;
  /** Re-send a failed optimistic message by nonce. */
  retry: (nonce: string) => Promise<Message>;
  /** Drop a failed optimistic message by nonce. */
  discard: (nonce: string) => void;
}

export interface MemberView {
  member: Member;
  user: User | undefined;
  /** Sorted by position, highest first. */
  roles: Role[];
  presence: Presence;
}

export interface GuildView {
  guild: Guild | undefined;
  roles: Role[];
}

export interface MembersApi {
  members: MemberView[];
  load: () => Promise<void>;
}

const NONE: string[] = [];
const offlineCache = new Map<string, Presence>();
const offline = (id: string) => {
  let p = offlineCache.get(id);
  if (!p) offlineCache.set(id, (p = { user_id: id, status: 'offline', activities: [] }));
  return p;
};

export const useConnectionStatus = (): ConnectionStatus => useStore((s) => s.connection);

export const useCurrentUser = (): User | null =>
  useStore((s) => (s.currentUserId ? (s.users[s.currentUserId] ?? null) : null));

/** Guilds in READY order. */
export function useGuilds(): Guild[] {
  const guilds = useStore((s) => s.guilds);
  const order = useStore((s) => s.guildOrder);
  return useMemo(
    () => order.map((id) => guilds[id]).filter((g): g is Guild => g !== undefined),
    [guilds, order],
  );
}

export function useGuild(guildId: string | undefined): GuildView {
  const guild = useStore((s) => (guildId ? s.guilds[guildId] : undefined));
  const roles = useStore((s) => s.roles);
  const ids = useStore((s) => (guildId ? s.roleIdsByGuild[guildId] : undefined)) ?? NONE;
  const list = useMemo(
    () =>
      ids
        .map((id) => roles[id])
        .filter((r): r is Role => r !== undefined)
        .sort((a, b) => b.position - a.position),
    [ids, roles],
  );
  return useMemo(() => ({ guild, roles: list }), [guild, list]);
}

/** Channels of a guild sorted by position then id. Pass '@me' for DMs (newest activity first). */
export function useChannels(guildId: string | undefined): Channel[] {
  const channels = useStore((s) => s.channels);
  const ids = useStore((s) => (guildId ? s.channelIdsByGuild[guildId] : undefined)) ?? NONE;
  return useMemo(() => {
    const list = ids.map((id) => channels[id]).filter((c): c is Channel => c !== undefined);
    if (guildId === DM_GUILD) {
      return list.sort((a, b) => cmpId(b.last_message_id ?? '0', a.last_message_id ?? '0'));
    }
    return list.sort((a, b) => (a.position ?? 0) - (b.position ?? 0) || cmpId(a.id, b.id));
  }, [ids, channels, guildId]);
}

export const useChannel = (channelId: string | undefined): Channel | undefined =>
  useStore((s) => (channelId ? s.channels[channelId] : undefined));

export function useMessages(channelId: string | undefined): MessagesApi {
  const window = useStore((s) => (channelId ? s.windows[channelId] : undefined));
  const byId = useStore((s) => s.messages);
  const loadInitialMessages = useStore((s) => s.loadInitialMessages);
  const loadOlderMessages = useStore((s) => s.loadOlderMessages);
  const sendMessage = useStore((s) => s.sendMessage);
  const retryMessage = useStore((s) => s.retryMessage);
  const discardMessage = useStore((s) => s.discardMessage);

  const hasWindow = window !== undefined;
  const stale = window?.stale === true;
  useEffect(() => {
    if (channelId && (!hasWindow || stale)) void loadInitialMessages(channelId);
  }, [channelId, hasWindow, stale, loadInitialMessages]);

  const messages = useMemo(() => {
    if (!window) return [];
    return [...window.ids, ...window.pending]
      .map((id) => byId[id])
      .filter((m): m is Message => m !== undefined);
  }, [window, byId]);

  const loadInitial = useCallback(
    () => (channelId ? loadInitialMessages(channelId) : Promise.resolve()),
    [channelId, loadInitialMessages],
  );
  const loadOlder = useCallback(
    () => (channelId ? loadOlderMessages(channelId) : Promise.resolve()),
    [channelId, loadOlderMessages],
  );
  const send = useCallback(
    (content: string) =>
      channelId ? sendMessage(channelId, content) : Promise.reject(new Error('No channel')),
    [channelId, sendMessage],
  );
  const retry = useCallback(
    (nonce: string) =>
      channelId ? retryMessage(channelId, nonce) : Promise.reject(new Error('No channel')),
    [channelId, retryMessage],
  );
  const discard = useCallback(
    (nonce: string) => {
      if (channelId) discardMessage(channelId, nonce);
    },
    [channelId, discardMessage],
  );

  return useMemo(
    () => ({
      messages,
      hasMoreBefore: window?.hasMoreBefore ?? false,
      loading: window?.loading ?? false,
      error: window?.error ?? null,
      loadInitial,
      loadOlder,
      send,
      retry,
      discard,
    }),
    [messages, window, loadInitial, loadOlder, send, retry, discard],
  );
}

export function useMembers(guildId: string | undefined): MembersApi {
  const ids = useStore((s) => (guildId ? s.memberIdsByGuild[guildId] : undefined)) ?? NONE;
  const members = useStore((s) => s.members);
  const users = useStore((s) => s.users);
  const roles = useStore((s) => s.roles);
  const presences = useStore((s) => s.presences);
  const loadMembers = useStore((s) => s.loadMembers);
  const views = useMemo(() => {
    if (!guildId) return [];
    const out: MemberView[] = [];
    for (const uid of ids) {
      const member = members[memberKey(guildId, uid)];
      if (!member) continue;
      out.push({
        member,
        user: users[uid],
        roles: member.roles
          .map((r) => roles[r])
          .filter((r): r is Role => r !== undefined)
          .sort((a, b) => b.position - a.position),
        presence: presences[uid] ?? offline(uid),
      });
    }
    return out;
  }, [guildId, ids, members, users, roles, presences]);
  const load = useCallback(
    () => (guildId ? loadMembers(guildId) : Promise.resolve()),
    [guildId, loadMembers],
  );
  return useMemo(() => ({ members: views, load }), [views, load]);
}

export function usePresence(userId: string | undefined): Presence {
  return useStore((s) => (userId ? s.presences[userId] : undefined)) ?? offline(userId ?? '');
}

/** Users currently typing in a channel (excluding the signed-in user). Re-renders on expiry. */
export function useTyping(channelId: string | undefined): User[] {
  const entries = useStore((s) => (channelId ? s.typing[channelId] : undefined));
  const users = useStore((s) => s.users);
  const pruneTyping = useStore((s) => s.pruneTyping);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!entries?.length) return;
    const t = Date.now();
    setNow(t);
    const next = Math.min(...entries.map((e) => e.expires_at));
    const timer = setTimeout(
      () => {
        pruneTyping();
        setNow(Date.now());
      },
      Math.max(0, next - t) + 5,
    );
    return () => clearTimeout(timer);
  }, [entries, pruneTyping]);

  return useMemo(
    () =>
      (entries ?? [])
        .filter((e) => e.expires_at > now)
        .map((e) => users[e.user_id])
        .filter((u): u is User => u !== undefined),
    [entries, users, now],
  );
}

export const useReadState = (channelId: string | undefined): ReadState | undefined =>
  useStore((s) => (channelId ? s.readStates[channelId] : undefined));
