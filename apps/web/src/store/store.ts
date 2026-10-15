// FND-API-008: Zustand store. Data is normalized by id; per-channel message windows hold order.
import { create } from 'zustand';
import {
  type StoreData,
  removeFromWindowState,
  cmpId,
  emptyData,
  emptyWindow,
  mergeIds,
  pendingId,
  putServerMessage,
  reduceDispatch,
} from './reducer';
import * as rest from './rest';
import { emitDispatch } from './events';
import type { Channel, ConnectionStatus, Message, MessageReaction, MessageWindow } from './types';

/** Minimum gap between two read-state acks for one channel (Q128). */
export const ACK_THROTTLE_MS = 1000;
const ackTimers = new Map<string, ReturnType<typeof setTimeout>>();
const lastAckAt = new Map<string, number>();
/** Channels the viewer marked unread: acks stay off until they leave the channel (LNG-MSGUI-026). */
const heldAcks = new Set<string>();

/** Extras of messages still being delivered, kept so a retry re-sends them. */
const sentExtras = new Map<string, rest.PostExtras>();

const reactionKey = (r: MessageReaction) => r.emoji.name ?? r.emoji.id ?? '';

/** In-flight de-duplication for keyed loads (the promise is shared until it settles). */
const inflight = new Map<string, Promise<void>>();
function once(key: string, run: () => Promise<void>): Promise<void> {
  const pending = inflight.get(key);
  if (pending) return pending;
  const p = run().finally(() => inflight.delete(key));
  inflight.set(key, p);
  return p;
}

/** Optimistic poll vote: single-choice polls move the vote, multi-choice add or remove one answer. */
function withVote(m: Message, answerId: number, add: boolean): Message {
  const poll = m.poll;
  if (!poll?.results) return m;
  const counts = poll.results.answer_counts.map((c) => {
    if (c.id === answerId) {
      return { ...c, count: Math.max(0, c.count + (add ? 1 : -1)), me_voted: add };
    }
    if (add && !poll.allow_multiselect && c.me_voted) {
      return { ...c, count: Math.max(0, c.count - 1), me_voted: false };
    }
    return c;
  });
  return { ...m, poll: { ...poll, results: { ...poll.results, answer_counts: counts } } };
}

/** Optimistic reaction change; the server result is not needed to render the toggle. */
function withReaction(m: Message, emoji: string, add: boolean): Message {
  const list = [...(m.reactions ?? [])];
  const i = list.findIndex((r) => reactionKey(r) === emoji);
  const cur = list[i];
  if (add) {
    if (cur) list[i] = { ...cur, count: cur.count + 1, me: true };
    else list.push({ emoji: { id: null, name: emoji }, count: 1, me: true });
  } else if (cur) {
    if (cur.count <= 1) list.splice(i, 1);
    else list[i] = { ...cur, count: cur.count - 1, me: false };
  }
  return { ...m, reactions: list };
}

export interface StoreActions {
  setConnection: (s: ConnectionStatus) => void;
  /** Feed one gateway dispatch (called by the gateway client). */
  applyDispatch: (event: string, data: unknown) => void;
  /** Drop expired typing entries. */
  pruneTyping: (now?: number) => void;
  loadGuilds: () => Promise<void>;
  /** Fetch guild, channels and roles over REST (for guilds not yet hydrated by GUILD_CREATE). */
  loadGuild: (guildId: string) => Promise<void>;
  loadMembers: (guildId: string) => Promise<void>;
  loadInitialMessages: (channelId: string) => Promise<void>;
  loadOlderMessages: (channelId: string) => Promise<void>;
  /** In a jumped-to window: fetch the page after the newest loaded message (LNG-MSGUI-001). */
  loadNewerMessages: (channelId: string) => Promise<void>;
  /** Optimistic send; `replyToId` makes it a reply. */
  sendMessage: (
    channelId: string,
    content: string,
    replyToId?: string | null,
    extras?: rest.PostExtras,
  ) => Promise<Message>;
  retryMessage: (channelId: string, nonce: string) => Promise<Message>;
  discardMessage: (channelId: string, nonce: string) => void;
  editMessage: (channelId: string, messageId: string, content: string) => Promise<Message>;
  /** DELETE the message; it leaves the window at once (rolled back on failure). */
  deleteMessage: (channelId: string, messageId: string) => Promise<void>;
  /** Drops a message from this client only (ephemeral app replies: "Dismiss message"). */
  dismissLocalMessage: (channelId: string, messageId: string) => void;
  pinMessage: (channelId: string, messageId: string, pinned: boolean) => Promise<void>;
  /** Cast or withdraw the viewer's poll vote (optimistic). */
  votePoll: (channelId: string, messageId: string, answerId: number, add: boolean) => Promise<void>;
  endPoll: (channelId: string, messageId: string) => Promise<void>;
  /** Forward a message to another channel. */
  forwardMessage: (
    targetChannelId: string,
    source: { channelId: string; messageId: string; guildId?: string },
  ) => Promise<Message>;
  /**
   * Mark the channel read up to its newest loaded message. Throttled to one POST per second per
   * channel; a call inside the window is sent once the window ends (trailing).
   */
  ackChannel: (channelId: string) => void;
  /**
   * Mark Unread: set the read marker to the message before `messageId` (optimistic) and hold automatic
   * acks for the channel. Resolves with the id the marker was set to.
   */
  markUnread: (channelId: string, messageId: string) => Promise<string>;
  /** Let automatic acks resume (called when the viewer leaves the channel). */
  releaseAck: (channelId: string) => void;
  /** Add or remove the signed-in user's reaction (optimistic; rolled back on failure). */
  toggleReaction: (channelId: string, messageId: string, emoji: string) => Promise<void>;
  /** POST /guilds, then hydrate the new guild (channels, roles) into the store. */
  createGuild: (name: string, template?: string) => Promise<rest.GuildWithChannels>;
  /** POST /invites/{code}, then load the joined guild. Resolves with the accepted invite. */
  acceptInvite: (code: string) => Promise<rest.Invite>;
  /** POST /guilds/{id}/channels; the channel is added to the store without waiting for the gateway. */
  createChannel: (guildId: string, body: rest.CreateChannelBody) => Promise<Channel>;
  /** Make sure `messageId` is in the channel window, loading the page around it when it is not. */
  loadAroundMessage: (channelId: string, messageId: string) => Promise<void>;
  /** Drop the loaded window and reload the newest page (leaves a "jump to message" view). */
  jumpToPresent: (channelId: string) => Promise<void>;
}

export interface StoreState extends StoreData, StoreActions {
  connection: ConnectionStatus;
}

let nonceCounter = 0;
const newNonce = () => `${Date.now().toString(36)}${(nonceCounter++).toString(36)}`;

export const useStore = create<StoreState>()((set, get) => {
  const patchWindow = (channelId: string, patch: Partial<MessageWindow>) =>
    set((s) => ({
      windows: {
        ...s.windows,
        [channelId]: { ...(s.windows[channelId] ?? emptyWindow()), ...patch },
      },
    }));

  const ingest = (s: StoreState, list: Message[]) => {
    const users = { ...s.users };
    const messages = { ...s.messages };
    for (const m of list) {
      users[m.author.id] = { ...users[m.author.id], ...m.author };
      messages[m.id] = { ...messages[m.id], ...m };
    }
    return { users, messages };
  };

  const deliver = async (
    channelId: string,
    content: string,
    nonce: string,
    replyToId: string | null,
    extras?: rest.PostExtras,
  ): Promise<Message> => {
    const pid = pendingId(nonce);
    if (extras) sentExtras.set(nonce, extras);
    try {
      const msg = await rest.postMessage(channelId, content, nonce, replyToId, extras);
      sentExtras.delete(nonce);
      // The gateway may already have reconciled; putServerMessage is idempotent.
      set((s) => {
        const w = s.windows[channelId];
        const messages = { ...s.messages };
        delete messages[pid];
        const base: StoreData = {
          ...s,
          messages,
          windows: w
            ? { ...s.windows, [channelId]: { ...w, pending: w.pending.filter((p) => p !== pid) } }
            : s.windows,
        };
        return putServerMessage(base, { ...msg, nonce });
      });
      return msg;
    } catch (e) {
      set((s) => {
        const m = s.messages[pid];
        if (!m) return {};
        const error = e instanceof Error ? e.message : 'failed';
        return {
          messages: { ...s.messages, [pid]: { ...m, local: { status: 'failed', nonce, error } } },
        };
      });
      throw e;
    }
  };

  return {
    ...emptyData(),
    connection: 'idle',
    setConnection: (connection) => set({ connection }),
    applyDispatch: (event, data) => {
      set((s) => reduceDispatch(s, event, data));
      emitDispatch(event, data);
    },
    pruneTyping: (now = Date.now()) =>
      set((s) => {
        let changed = false;
        const typing: StoreData['typing'] = {};
        for (const [cid, list] of Object.entries(s.typing)) {
          const live = list.filter((t) => t.expires_at > now);
          if (live.length !== list.length) changed = true;
          if (live.length) typing[cid] = live;
        }
        return changed ? { typing } : {};
      }),

    loadGuilds: async () => {
      const list = await rest.fetchGuilds();
      set((s) => {
        const guilds = { ...s.guilds };
        for (const g of list) guilds[g.id] = { ...guilds[g.id], ...g, unavailable: false };
        return { guilds, guildOrder: [...new Set([...list.map((g) => g.id), ...s.guildOrder])] };
      });
    },

    // Several Lounge components may ask for the same guild in one commit: share one request.
    loadGuild: (guildId) =>
      once(`guild:${guildId}`, async () => {
        const [guild, channels, roles] = await Promise.all([
          rest.fetchGuild(guildId),
          rest.fetchChannels(guildId),
          rest.fetchRoles(guildId),
        ]);
        get().applyDispatch('GUILD_CREATE', { ...guild, channels, roles });
      }),

    loadMembers: (guildId) =>
      once(`members:${guildId}`, async () => {
        const members = await rest.fetchMembers(guildId);
        get().applyDispatch('GUILD_MEMBERS_CHUNK', { guild_id: guildId, members });
      }),

    loadInitialMessages: async (channelId) => {
      const w = get().windows[channelId];
      if (w?.loading || (w && w.ids.length > 0)) return;
      patchWindow(channelId, { loading: true, error: null });
      try {
        const list = await rest.fetchMessages(channelId);
        set((s) => {
          const cur = s.windows[channelId] ?? emptyWindow();
          return {
            ...ingest(s, list),
            windows: {
              ...s.windows,
              [channelId]: {
                ...cur,
                ids: mergeIds(
                  cur.ids,
                  list.map((m) => m.id),
                ),
                hasMoreBefore: list.length >= rest.PAGE_SIZE,
                atLatest: true,
                loading: false,
                error: null,
              },
            },
          };
        });
      } catch (e) {
        patchWindow(channelId, {
          loading: false,
          error: e instanceof Error ? e.message : 'Failed to load messages',
        });
      }
    },

    loadOlderMessages: async (channelId) => {
      const w = get().windows[channelId];
      const oldest = w?.ids[0];
      if (!w || w.loading || !w.hasMoreBefore || !oldest) return;
      patchWindow(channelId, { loading: true, error: null });
      try {
        const list = await rest.fetchMessages(channelId, { before: oldest });
        set((s) => {
          const cur = s.windows[channelId] ?? emptyWindow();
          return {
            ...ingest(s, list),
            windows: {
              ...s.windows,
              [channelId]: {
                ...cur,
                ids: mergeIds(
                  list.map((m) => m.id),
                  cur.ids,
                ),
                hasMoreBefore: list.length >= rest.PAGE_SIZE,
                loading: false,
              },
            },
          };
        });
      } catch (e) {
        patchWindow(channelId, {
          loading: false,
          error: e instanceof Error ? e.message : 'Failed to load messages',
        });
      }
    },

    loadNewerMessages: async (channelId) => {
      const w = get().windows[channelId];
      const newest = w?.ids.at(-1);
      if (!w || w.atLatest || w.loading || !newest) return;
      patchWindow(channelId, { loading: true, error: null });
      try {
        const list = await rest.fetchMessages(channelId, { after: newest });
        set((s) => {
          const cur = s.windows[channelId] ?? emptyWindow();
          return {
            ...ingest(s, list),
            windows: {
              ...s.windows,
              [channelId]: {
                ...cur,
                ids: mergeIds(
                  cur.ids,
                  list.map((m) => m.id),
                ),
                // A short page means the newest message is loaded: live appends resume.
                atLatest: list.length < rest.PAGE_SIZE,
                loading: false,
              },
            },
          };
        });
      } catch (e) {
        patchWindow(channelId, {
          loading: false,
          error: e instanceof Error ? e.message : 'Failed to load messages',
        });
      }
    },

    sendMessage: (channelId, content, replyToId = null, extras) => {
      const s = get();
      const author = s.currentUserId ? s.users[s.currentUserId] : undefined;
      if (!author) return Promise.reject(new Error('Not signed in'));
      const nonce = newNonce();
      const pid = pendingId(nonce);
      const ch = s.channels[channelId];
      const optimistic: Message = {
        id: pid,
        channel_id: channelId,
        ...(ch?.guild_id ? { guild_id: ch.guild_id } : {}),
        author,
        content,
        timestamp: new Date().toISOString(),
        nonce,
        ...(replyToId ? { message_reference: { message_id: replyToId } } : {}),
        local: { status: 'sending', nonce },
      };
      set((st) => {
        const w = st.windows[channelId] ?? emptyWindow();
        return {
          messages: { ...st.messages, [pid]: optimistic },
          windows: { ...st.windows, [channelId]: { ...w, pending: [...w.pending, pid] } },
        };
      });
      return deliver(channelId, content, nonce, replyToId, extras);
    },

    retryMessage: (channelId, nonce) => {
      const m = get().messages[pendingId(nonce)];
      if (!m || m.local?.status !== 'failed') return Promise.reject(new Error('Nothing to retry'));
      set((st) => ({
        messages: { ...st.messages, [m.id]: { ...m, local: { status: 'sending', nonce } } },
      }));
      return deliver(
        channelId,
        m.content,
        nonce,
        m.message_reference?.message_id ?? null,
        sentExtras.get(nonce),
      );
    },

    editMessage: async (channelId, messageId, content) => {
      const msg = await rest.editMessage(channelId, messageId, content);
      set((s) => putServerMessage(s, msg));
      return msg;
    },

    dismissLocalMessage: (channelId, messageId) => {
      set((s) => removeFromWindowState(s, channelId, messageId));
    },

    deleteMessage: async (channelId, messageId) => {
      const before = get().messages[messageId];
      const win = get().windows[channelId];
      set((s) => removeFromWindowState(s, channelId, messageId));
      try {
        await rest.deleteMessage(channelId, messageId);
      } catch (e) {
        if (before)
          set((s) => ({
            messages: { ...s.messages, [messageId]: before },
            windows: win ? { ...s.windows, [channelId]: win } : s.windows,
          }));
        throw e;
      }
    },

    pinMessage: async (channelId, messageId, pinned) => {
      const before = get().messages[messageId];
      if (!before) return;
      set((s) => ({ messages: { ...s.messages, [messageId]: { ...before, pinned } } }));
      try {
        if (pinned) await rest.pinMessage(channelId, messageId);
        else await rest.unpinMessage(channelId, messageId);
      } catch (e) {
        set((s) => ({ messages: { ...s.messages, [messageId]: before } }));
        throw e;
      }
    },

    votePoll: async (channelId, messageId, answerId, add) => {
      const before = get().messages[messageId];
      if (!before?.poll) return;
      set((s) => ({
        messages: { ...s.messages, [messageId]: withVote(before, answerId, add) },
      }));
      try {
        if (add) await rest.votePoll(channelId, messageId, answerId);
        else await rest.unvotePoll(channelId, messageId, answerId);
      } catch (e) {
        set((s) => ({ messages: { ...s.messages, [messageId]: before } }));
        throw e;
      }
    },

    endPoll: async (channelId, messageId) => {
      await rest.endPoll(channelId, messageId);
    },

    forwardMessage: (targetChannelId, source) =>
      rest.forwardMessage(targetChannelId, source, newNonce()).then((msg) => {
        set((s) => putServerMessage(s, msg));
        return msg;
      }),

    markUnread: async (channelId, messageId) => {
      const ids = get().windows[channelId]?.ids ?? [];
      const at = ids.indexOf(messageId);
      // The loaded window may start at the message: step back one snowflake instead.
      const before =
        at > 0 ? ids[at - 1]! : /^\d+$/.test(messageId) ? (BigInt(messageId) - 1n).toString() : '0';
      heldAcks.add(channelId);
      set((st) => ({
        readStates: {
          ...st.readStates,
          [channelId]: { channel_id: channelId, last_message_id: before, mention_count: 0 },
        },
      }));
      await rest.markMessageUnread(channelId, before);
      return before;
    },

    releaseAck: (channelId) => {
      heldAcks.delete(channelId);
    },

    ackChannel: (channelId) => {
      if (heldAcks.has(channelId)) return;
      const fire = () => {
        ackTimers.delete(channelId);
        lastAckAt.set(channelId, Date.now());
        const s = get();
        const newest = s.windows[channelId]?.ids.at(-1);
        const read = s.readStates[channelId]?.last_message_id ?? null;
        if (!newest || (read && cmpId(read, newest) >= 0)) return;
        set((st) => ({
          readStates: {
            ...st.readStates,
            [channelId]: { channel_id: channelId, last_message_id: newest, mention_count: 0 },
          },
        }));
        rest.ackMessage(channelId, newest).catch(() => {
          /* read state is re-sent on the next view; a failed ack is not user-visible */
        });
      };
      const wait = (lastAckAt.get(channelId) ?? 0) + ACK_THROTTLE_MS - Date.now();
      if (wait <= 0) fire();
      else if (!ackTimers.has(channelId)) ackTimers.set(channelId, setTimeout(fire, wait));
    },

    toggleReaction: async (channelId, messageId, emoji) => {
      const before = get().messages[messageId];
      if (!before) return;
      const mine = before.reactions?.some((r) => r.me && reactionKey(r) === emoji) ?? false;
      set((s) => ({
        messages: { ...s.messages, [messageId]: withReaction(before, emoji, !mine) },
      }));
      try {
        if (mine) await rest.removeReaction(channelId, messageId, emoji);
        else await rest.addReaction(channelId, messageId, emoji);
      } catch {
        set((s) => ({ messages: { ...s.messages, [messageId]: before } }));
      }
    },

    createGuild: async (name, template) => {
      const guild = await rest.createGuild(name, template);
      get().applyDispatch('GUILD_CREATE', guild);
      return guild;
    },

    acceptInvite: async (code) => {
      const invite = await rest.acceptInvite(code);
      // The gateway also delivers GUILD_CREATE; loading here makes the guild usable immediately.
      // A group DM invite has no guild: the gateway delivers the new DM channel.
      if (invite.type === 0 && invite.guild)
        await get()
          .loadGuild(invite.guild.id)
          .catch(() => undefined);
      return invite;
    },

    createChannel: async (guildId, body) => {
      const channel = await rest.createChannel(guildId, body);
      get().applyDispatch('CHANNEL_CREATE', { ...channel, guild_id: channel.guild_id ?? guildId });
      return channel;
    },

    loadAroundMessage: async (channelId, messageId) => {
      const w = get().windows[channelId];
      if (w?.ids.includes(messageId)) return;
      patchWindow(channelId, { loading: true, error: null });
      try {
        const list = await rest.fetchMessagesAround(channelId, messageId);
        set((s) => ({
          ...ingest(s, list),
          windows: {
            ...s.windows,
            [channelId]: {
              ...emptyWindow(),
              ids: list.map((m) => m.id),
              hasMoreBefore: list.length > 0,
              atLatest: false,
            },
          },
        }));
      } catch (e) {
        patchWindow(channelId, {
          loading: false,
          error: e instanceof Error ? e.message : 'Failed to load messages',
        });
      }
    },

    jumpToPresent: async (channelId) => {
      set((s) => ({ windows: { ...s.windows, [channelId]: emptyWindow() } }));
      await get().loadInitialMessages(channelId);
    },

    discardMessage: (channelId, nonce) =>
      set((s) => {
        const pid = pendingId(nonce);
        const messages = { ...s.messages };
        delete messages[pid];
        const w = s.windows[channelId];
        return {
          messages,
          ...(w
            ? {
                windows: {
                  ...s.windows,
                  [channelId]: { ...w, pending: w.pending.filter((p) => p !== pid) },
                },
              }
            : {}),
        };
      }),
  };
});

export function resetStore(): void {
  for (const t of ackTimers.values()) clearTimeout(t);
  ackTimers.clear();
  lastAckAt.clear();
  heldAcks.clear();
  useStore.setState({ ...emptyData(), connection: 'idle' });
}
