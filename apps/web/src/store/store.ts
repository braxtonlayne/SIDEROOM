// FND-API-008: Zustand store. Data is normalized by id; per-channel message windows hold order.
import { create } from 'zustand';
import {
  type StoreData,
  emptyData,
  emptyWindow,
  mergeIds,
  pendingId,
  putServerMessage,
  reduceDispatch,
} from './reducer';
import * as rest from './rest';
import type { ConnectionStatus, Message, MessageWindow } from './types';

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
  sendMessage: (channelId: string, content: string) => Promise<Message>;
  retryMessage: (channelId: string, nonce: string) => Promise<Message>;
  discardMessage: (channelId: string, nonce: string) => void;
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

  const deliver = async (channelId: string, content: string, nonce: string): Promise<Message> => {
    const pid = pendingId(nonce);
    try {
      const msg = await rest.postMessage(channelId, content, nonce);
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
    applyDispatch: (event, data) => set((s) => reduceDispatch(s, event, data)),
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

    loadGuild: async (guildId) => {
      const [guild, channels, roles] = await Promise.all([
        rest.fetchGuild(guildId),
        rest.fetchChannels(guildId),
        rest.fetchRoles(guildId),
      ]);
      get().applyDispatch('GUILD_CREATE', { ...guild, channels, roles });
    },

    loadMembers: async (guildId) => {
      const members = await rest.fetchMembers(guildId);
      get().applyDispatch('GUILD_MEMBERS_CHUNK', { guild_id: guildId, members });
    },

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

    sendMessage: (channelId, content) => {
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
        local: { status: 'sending', nonce },
      };
      set((st) => {
        const w = st.windows[channelId] ?? emptyWindow();
        return {
          messages: { ...st.messages, [pid]: optimistic },
          windows: { ...st.windows, [channelId]: { ...w, pending: [...w.pending, pid] } },
        };
      });
      return deliver(channelId, content, nonce);
    },

    retryMessage: (channelId, nonce) => {
      const m = get().messages[pendingId(nonce)];
      if (!m || m.local?.status !== 'failed') return Promise.reject(new Error('Nothing to retry'));
      set((st) => ({
        messages: { ...st.messages, [m.id]: { ...m, local: { status: 'sending', nonce } } },
      }));
      return deliver(channelId, m.content, nonce);
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
  useStore.setState({ ...emptyData(), connection: 'idle' });
}
