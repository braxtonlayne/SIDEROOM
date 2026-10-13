/**
 * Friends, requests and message requests for the Home screen. The server has no relationship gateway events
 * yet (LNG-FRD-003), so this store loads over REST and every action refreshes the affected list.
 */
import { create } from 'zustand';
import { apiFetch } from '../../api/client';
import { useStore } from '../../store';
import { REL, listRelationships, type DmObject, type RelationshipRow } from './api';

export interface SocialState {
  relationships: RelationshipRow[];
  /** Conversations from people who are not friends yet (`is_message_request`). */
  messageRequests: DmObject[];
  status: 'idle' | 'loading' | 'ready' | 'error';
  messageRequestsStatus: 'idle' | 'loading' | 'ready' | 'error';
  loadRelationships(): Promise<void>;
  /**
   * Loads the DM list over REST. READY carries only `recipient_ids` for DMs, so this is what puts the
   * people (and so the names) into the store; requests are kept apart from the main list.
   */
  loadDms(): Promise<void>;
  /** Drop a request locally right after answering it. */
  forgetMessageRequest(channelId: string): void;
  reset(): void;
}

export const useSocial = create<SocialState>()((set, get) => ({
  relationships: [],
  messageRequests: [],
  status: 'idle',
  messageRequestsStatus: 'idle',
  async loadRelationships() {
    if (get().status === 'loading') return;
    set({ status: 'loading' });
    try {
      const rows = await listRelationships();
      set({ relationships: rows, status: 'ready' });
    } catch {
      set({ status: 'error' });
    }
  },
  async loadDms() {
    if (get().messageRequestsStatus === 'loading') return;
    set({ messageRequestsStatus: 'loading' });
    try {
      const list = await apiFetch<DmObject[]>('/users/@me/channels', { retries: 0 });
      const store = useStore.getState();
      for (const d of list) if (!d.is_message_request) store.applyDispatch('CHANNEL_CREATE', d);
      set({
        messageRequests: list.filter((d) => d.is_message_request),
        messageRequestsStatus: 'ready',
      });
    } catch {
      set({ messageRequestsStatus: 'error' });
    }
  },
  forgetMessageRequest: (channelId) =>
    set((s) => ({ messageRequests: s.messageRequests.filter((d) => d.id !== channelId) })),
  reset: () =>
    set({
      relationships: [],
      messageRequests: [],
      status: 'idle',
      messageRequestsStatus: 'idle',
    }),
}));

export const friendsOf = (rows: readonly RelationshipRow[]) =>
  rows.filter((r) => r.type === REL.FRIEND);
export const incomingOf = (rows: readonly RelationshipRow[]) =>
  rows.filter((r) => r.type === REL.INCOMING);
export const outgoingOf = (rows: readonly RelationshipRow[]) =>
  rows.filter((r) => r.type === REL.OUTGOING);

/** Display name for a user object off the wire. */
export const wireName = (u: { username: string; global_name?: string | null }) =>
  u.global_name || u.username;
