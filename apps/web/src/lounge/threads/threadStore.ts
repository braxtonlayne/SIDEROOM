/**
 * Threads for the Lounge (phase 1 endpoints: `POST /channels/{c}/messages/{m}/threads`, `GET
 * /guilds/{g}/threads/active`, `GET /channels/{c}/threads/archived/public`, `PUT/DELETE
 * /channels/{t}/thread-members/@me`). Threads are not part of the guild channel list, so they live in their own
 * store, kept current from `GUILD_CREATE.threads`, `THREAD_LIST_SYNC` and `THREAD_CREATE/UPDATE/DELETE`.
 * Their messages are ordinary channel messages and use the main store's windows.
 */
import { create } from 'zustand';
import { apiFetch } from '../../api/client';
import { withReauth } from '../../admin/reauth';
import { onDispatch } from '../../store/events';

export interface ThreadInfo {
  id: string;
  parentId: string;
  guildId: string;
  name: string;
  ownerId: string | null;
  archived: boolean;
  locked: boolean;
  messageCount: number;
  memberCount: number;
  /** The viewer is a member (follows the thread). */
  joined: boolean;
  lastMessageId: string | null;
  /** Archive time, used to page archived lists. */
  archiveTimestamp: string | null;
  /** Private thread (channel type 12): only invited members see it. */
  isPrivate: boolean;
  /** Seconds between messages for members without Manage Messages/Threads. */
  slowmode: number;
  /** Minutes of inactivity before the thread archives itself. */
  autoArchive: number;
  /** Forum posts: ids of the tags applied to the post. */
  appliedTags: string[];
  /** Forum posts: the thread owner pinned it (flag 2). */
  pinned: boolean;
}

interface WireThread {
  id: string;
  parent_id?: string | null;
  guild_id?: string | null;
  name?: string | null;
  owner_id?: string | null;
  last_message_id?: string | null;
  message_count?: number | null;
  member_count?: number | null;
  member?: unknown;
  type?: number;
  rate_limit_per_user?: number | null;
  flags?: number | null;
  applied_tags?: string[] | null;
  thread_metadata?: {
    archived?: boolean;
    locked?: boolean;
    archive_timestamp?: string;
    auto_archive_duration?: number;
  } | null;
}

export function toThread(w: WireThread, prev?: ThreadInfo): ThreadInfo | null {
  if (!w.id || !w.parent_id) return null;
  const meta = w.thread_metadata;
  return {
    id: w.id,
    parentId: w.parent_id,
    guildId: w.guild_id ?? prev?.guildId ?? '',
    name: w.name ?? prev?.name ?? 'thread',
    ownerId: w.owner_id ?? prev?.ownerId ?? null,
    archived: meta?.archived ?? prev?.archived ?? false,
    locked: meta?.locked ?? prev?.locked ?? false,
    messageCount: w.message_count ?? prev?.messageCount ?? 0,
    memberCount: w.member_count ?? prev?.memberCount ?? 0,
    joined: 'member' in w ? !!w.member : (prev?.joined ?? false),
    lastMessageId: w.last_message_id ?? prev?.lastMessageId ?? null,
    archiveTimestamp: meta?.archive_timestamp ?? prev?.archiveTimestamp ?? null,
    isPrivate: w.type === undefined ? (prev?.isPrivate ?? false) : w.type === 12,
    slowmode: w.rate_limit_per_user ?? prev?.slowmode ?? 0,
    autoArchive: meta?.auto_archive_duration ?? prev?.autoArchive ?? 1440,
    appliedTags: w.applied_tags ?? prev?.appliedTags ?? [],
    pinned:
      w.flags === undefined || w.flags === null ? (prev?.pinned ?? false) : (w.flags & 2) !== 0,
  };
}

interface ThreadList {
  threads: WireThread[];
  members?: { id: string }[];
  has_more?: boolean;
}

export interface ThreadsState {
  byId: Record<string, ThreadInfo>;
  /** Guilds whose active threads were fetched over REST. */
  loadedGuilds: Record<string, boolean>;
  upsert(list: WireThread[], opts?: { joinedIds?: ReadonlySet<string> }): void;
  remove(id: string): void;
  loadActive(guildId: string): Promise<void>;
  /** Archived public threads of a channel; resolves with whether more pages exist. */
  loadArchived(channelId: string, before?: string): Promise<boolean>;
  /** Start a thread from a message (the thread id is the message id). */
  createFromMessage(
    channelId: string,
    messageId: string,
    body: { name: string; auto_archive_duration?: number },
  ): Promise<ThreadInfo>;
  /** Start a thread that is not attached to a message (`POST /channels/{c}/threads`). */
  createStandalone(
    channelId: string,
    body: { name: string; auto_archive_duration?: number; private?: boolean },
  ): Promise<ThreadInfo>;
  /** Thread settings (`PATCH /channels/{thread}`). */
  update(
    threadId: string,
    patch: {
      name?: string;
      rate_limit_per_user?: number;
      auto_archive_duration?: number;
      locked?: boolean;
      archived?: boolean;
    },
  ): Promise<void>;
  /** Delete the thread (`DELETE /channels/{thread}`). */
  destroy(threadId: string): Promise<void>;
  join(threadId: string): Promise<void>;
  leave(threadId: string): Promise<void>;
  reset(): void;
}

export const useThreads = create<ThreadsState>()((set, get) => ({
  byId: {},
  loadedGuilds: {},
  upsert: (list, opts) =>
    set((s) => {
      const byId = { ...s.byId };
      for (const w of list) {
        const t = toThread(w, byId[w.id]);
        if (!t) continue;
        byId[t.id] = opts?.joinedIds?.has(t.id) ? { ...t, joined: true } : t;
      }
      return { byId };
    }),
  remove: (id) =>
    set((s) => {
      if (!(id in s.byId)) return s;
      const byId = { ...s.byId };
      delete byId[id];
      return { byId };
    }),
  async loadActive(guildId) {
    const res = await apiFetch<ThreadList>(`/guilds/${guildId}/threads/active`, { retries: 0 });
    get().upsert(res.threads, { joinedIds: new Set((res.members ?? []).map((m) => m.id)) });
    set((s) => ({ loadedGuilds: { ...s.loadedGuilds, [guildId]: true } }));
  },
  async loadArchived(channelId, before) {
    const q = new URLSearchParams({ limit: '25' });
    if (before) q.set('before', before);
    const res = await apiFetch<ThreadList>(`/channels/${channelId}/threads/archived/public?${q}`, {
      retries: 0,
    });
    get().upsert(res.threads, { joinedIds: new Set((res.members ?? []).map((m) => m.id)) });
    return !!res.has_more;
  },
  async createFromMessage(channelId, messageId, body) {
    const w = await apiFetch<WireThread>(`/channels/${channelId}/messages/${messageId}/threads`, {
      method: 'POST',
      json: body,
      retries: 0,
    });
    get().upsert([{ ...w, member: w.member ?? {} }]);
    const t = get().byId[w.id];
    if (!t) throw new Error('The thread could not be created.');
    return t;
  },
  async createStandalone(channelId, body) {
    const { private: isPrivate, ...rest } = body;
    const w = await apiFetch<WireThread>(`/channels/${channelId}/threads`, {
      method: 'POST',
      json: { ...rest, type: isPrivate ? 12 : 11, ...(isPrivate ? { invitable: true } : {}) },
      retries: 0,
    });
    get().upsert([{ ...w, member: w.member ?? {} }]);
    const t = get().byId[w.id];
    if (!t) throw new Error('The thread could not be created.');
    return t;
  },
  async update(threadId, patch) {
    const w = await withReauth(() =>
      apiFetch<WireThread>(`/channels/${threadId}`, {
        method: 'PATCH',
        json: patch,
        retries: 0,
      }),
    );
    get().upsert([w]);
  },
  async destroy(threadId) {
    await withReauth(() =>
      apiFetch<void>(`/channels/${threadId}`, { method: 'DELETE', retries: 0 }),
    );
    get().remove(threadId);
  },
  async join(threadId) {
    await apiFetch<void>(`/channels/${threadId}/thread-members/@me`, { method: 'PUT', retries: 0 });
    set((s) => {
      const t = s.byId[threadId];
      return t ? { byId: { ...s.byId, [threadId]: { ...t, joined: true } } } : s;
    });
  },
  async leave(threadId) {
    await apiFetch<void>(`/channels/${threadId}/thread-members/@me`, {
      method: 'DELETE',
      retries: 0,
    });
    set((s) => {
      const t = s.byId[threadId];
      return t ? { byId: { ...s.byId, [threadId]: { ...t, joined: false } } } : s;
    });
  },
  reset: () => set({ byId: {}, loadedGuilds: {} }),
}));

/** Threads of one parent channel: active ones first (newest activity first), then archived. */
export function threadsOf(
  byId: Record<string, ThreadInfo>,
  parentId: string,
  opts: { archived?: boolean } = {},
): ThreadInfo[] {
  return Object.values(byId)
    .filter((t) => t.parentId === parentId && t.archived === !!opts.archived)
    .sort((a, b) => ((a.lastMessageId ?? a.id) < (b.lastMessageId ?? b.id) ? 1 : -1));
}

type Obj = Record<string, unknown>;

/**
 * The server stops counting here (`sr_chat::threads::MESSAGE_COUNT_CAP`, as in Discord): a thread of 400 messages
 * reports 50. A count at or over this is shown "50+", never raised and never lowered by a delete, because the true
 * number is not known until the next load.
 */
export const COUNT_CAP = 50;

const snow = (a: string, b: string) =>
  a.length !== b.length ? a.length - b.length : a < b ? -1 : a > b ? 1 : 0;

/**
 * A message created, or messages deleted, in a thread the store holds: the server's own rule, mirrored. Every
 * message stored in a thread counts one (the `message_sent` hook of `ThreadsDeps`, whatever its kind), a delete
 * of a visible message takes one off (never below 0). So no kind has to be told apart here.
 */
function countMessages(event: string, d: Obj): void {
  const id = typeof d.channel_id === 'string' ? d.channel_id : '';
  const t = id ? useThreads.getState().byId[id] : undefined;
  if (!t) return;
  let next = t.messageCount;
  let last = t.lastMessageId;
  if (event === 'MESSAGE_CREATE') {
    const mid = typeof d.id === 'string' ? d.id : '';
    // A message the thread already knows (a replay after a resync) is not counted twice. A message a moderator
    // restores (`restored: true`, D-723) is old by id and new to the thread: it counts, and the last message
    // moves only when the restored one is the newest.
    const newest = !t.lastMessageId || snow(mid, t.lastMessageId) > 0;
    if (!mid || (!newest && d.restored !== true)) return;
    if (newest) last = mid;
    if (next < COUNT_CAP) next += 1;
  } else if (next < COUNT_CAP) {
    const gone = event === 'MESSAGE_DELETE' ? 1 : Array.isArray(d.ids) ? d.ids.length : 0;
    next = Math.max(0, next - gone);
  }
  if (next === t.messageCount && last === t.lastMessageId) return;
  useThreads.setState((s) => ({
    byId: { ...s.byId, [id]: { ...t, messageCount: next, lastMessageId: last } },
  }));
}

/** The count of one thread, for a row to select (a number, so a change re-renders that thread's rows only). */
export function useThreadCount(id: string | undefined, fallback: number): number {
  return useThreads((s) => (id ? (s.byId[id]?.messageCount ?? fallback) : fallback));
}

/**
 * The thread ids the newest GUILD_CREATE / THREAD_LIST_SYNC of each guild named. A resync (D-646) ends with the
 * marker `SR_RESYNC`; the active threads of that guild that those two did not name are gone (deleted, or no longer
 * visible), and upserting alone would keep them.
 */
const named = new Map<string, Set<string>>();

/** Applies one gateway dispatch to the thread store. */
export function applyThreadDispatch(event: string, data: unknown): void {
  const d = (data ?? {}) as Obj;
  const st = useThreads.getState();
  switch (event) {
    case 'GUILD_CREATE':
      named.set(
        String(d.id),
        new Set(Array.isArray(d.threads) ? (d.threads as WireThread[]).map((t) => t.id) : []),
      );
      if (Array.isArray(d.threads)) st.upsert(d.threads as WireThread[]);
      return;
    case 'THREAD_LIST_SYNC':
      if (Array.isArray(d.threads)) {
        const gid = String(d.guild_id);
        const seen = named.get(gid) ?? new Set<string>();
        for (const t of d.threads as WireThread[]) seen.add(t.id);
        named.set(gid, seen);
        const joined = new Set(
          Array.isArray(d.members) ? (d.members as { id: string }[]).map((m) => m.id) : [],
        );
        st.upsert(d.threads as WireThread[], { joinedIds: joined });
      }
      return;
    case 'SR_RESYNC': {
      if (d.scope !== 'guild') return;
      const gid = String(d.guild_id);
      const keep = named.get(gid);
      named.delete(gid);
      if (!keep) return;
      useThreads.setState((s) => {
        const byId = { ...s.byId };
        let changed = false;
        for (const t of Object.values(s.byId)) {
          if (t.guildId === gid && !t.archived && !keep.has(t.id)) {
            delete byId[t.id];
            changed = true;
          }
        }
        return changed ? { byId } : s;
      });
      return;
    }
    case 'THREAD_CREATE':
    case 'THREAD_UPDATE':
      if (typeof d.id === 'string') st.upsert([d as unknown as WireThread]);
      return;
    case 'THREAD_DELETE':
      if (typeof d.id === 'string') st.remove(d.id);
      return;
    case 'MESSAGE_CREATE':
    case 'MESSAGE_DELETE':
    case 'MESSAGE_DELETE_BULK':
      countMessages(event, d);
      return;
    case 'THREAD_MEMBER_UPDATE':
      if (typeof d.id === 'string') {
        const id = d.id;
        useThreads.setState((s) => {
          const t = s.byId[id];
          return t ? { byId: { ...s.byId, [id]: { ...t, joined: true } } } : s;
        });
      }
      return;
  }
}

/** Mounts the dispatch subscription (the shell calls this once). */
export function startThreadsLive(): () => void {
  return onDispatch(applyThreadDispatch);
}
