/**
 * Friends, requests and message requests for the Home screen. Friends load over REST once; message requests come in READY
 * (full `recipients`, `is_message_request`, D-181). After that the
 * gateway keeps them current: `RELATIONSHIP_ADD/UPDATE/REMOVE` and DM `CHANNEL_CREATE/DELETE` are applied by
 * `startSocialLive` (D-183, LNG-FRD-003), so actions no longer re-fetch.
 */
import { create } from 'zustand';
import { useStore } from '../../store';
import { onDispatch } from '../../store/events';
import { REL, listRelationships, type DmObject, type RelationshipRow } from './api';

export interface SocialState {
  relationships: RelationshipRow[];
  /** Conversations from people who are not friends yet (`is_message_request`). */
  messageRequests: DmObject[];
  status: 'idle' | 'loading' | 'ready' | 'error';
  loadRelationships(): Promise<void>;
  /** Replace the message-request list (seeded from READY, kept current by CHANNEL_* dispatches). */
  setMessageRequests(list: DmObject[]): void;
  /** Drop a request locally right after answering it. */
  forgetMessageRequest(channelId: string): void;
  /** Apply one gateway dispatch (RELATIONSHIP_*, DM CHANNEL_CREATE/DELETE). */
  applyEvent(event: string, data: unknown): void;
  reset(): void;
}

export const useSocial = create<SocialState>()((set, get) => ({
  relationships: [],
  messageRequests: [],
  status: 'idle',
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
  setMessageRequests: (messageRequests) => set({ messageRequests }),
  applyEvent(event, data) {
    set((s) => {
      if (event.startsWith('RELATIONSHIP_')) {
        const relationships = applyRelationshipEvent(s.relationships, event, data);
        return relationships === s.relationships ? s : { relationships };
      }
      const messageRequests = applyDmEvent(s.messageRequests, event, data);
      return messageRequests === s.messageRequests ? s : { messageRequests };
    });
  },
  forgetMessageRequest: (channelId) => {
    useStore.setState((s) => ({
      messageRequestDms: s.messageRequestDms.filter((d) => d.id !== channelId),
    }));
    set((s) => ({ messageRequests: s.messageRequests.filter((d) => d.id !== channelId) }));
  },
  reset: () =>
    set({
      relationships: [],
      messageRequests: [],
      status: 'idle',
    }),
}));

type Obj = Record<string, unknown>;

/**
 * Pure: one RELATIONSHIP_ADD / UPDATE / REMOVE dispatch applied to the relationship rows. ADD upserts the
 * whole row (new request, accepted friend, block); UPDATE changes the nickname (and the type when sent);
 * REMOVE drops the row. Unknown ids and malformed payloads leave the list as it was.
 */
export function applyRelationshipEvent(
  rows: RelationshipRow[],
  event: string,
  data: unknown,
): RelationshipRow[] {
  const d = (data ?? {}) as Obj;
  const id = typeof d.id === 'string' ? d.id : '';
  if (!id) return rows;
  switch (event) {
    case 'RELATIONSHIP_ADD': {
      const user = d.user as RelationshipRow['user'] | undefined;
      const prev = rows.find((r) => r.id === id);
      const u = user ?? prev?.user;
      if (!u || typeof d.type !== 'number') return rows;
      const row: RelationshipRow = {
        id,
        type: d.type,
        nickname: (d.nickname as string | null | undefined) ?? null,
        since: typeof d.since === 'string' ? d.since : (prev?.since ?? ''),
        user: u,
      };
      return prev ? rows.map((r) => (r.id === id ? row : r)) : [...rows, row];
    }
    case 'RELATIONSHIP_UPDATE': {
      if (!rows.some((r) => r.id === id)) return rows;
      return rows.map((r) =>
        r.id === id
          ? {
              ...r,
              ...(typeof d.type === 'number' ? { type: d.type } : {}),
              ...('nickname' in d ? { nickname: (d.nickname as string | null) ?? null } : {}),
            }
          : r,
      );
    }
    case 'RELATIONSHIP_REMOVE':
      return rows.some((r) => r.id === id) ? rows.filter((r) => r.id !== id) : rows;
    default:
      return rows;
  }
}

/**
 * Pure: DM CHANNEL_CREATE / CHANNEL_DELETE applied to the message-request list. A request DM joins it, an
 * accepted one (CHANNEL_CREATE with `is_message_request` false) or a deleted one leaves it. The main DM list
 * is the store reducer's job.
 */
export function applyDmEvent(requests: DmObject[], event: string, data: unknown): DmObject[] {
  const d = (data ?? {}) as Partial<DmObject>;
  if (typeof d.id !== 'string' || (d.type !== 1 && d.type !== 3 && event !== 'CHANNEL_DELETE'))
    return requests;
  const has = requests.some((r) => r.id === d.id);
  if (event === 'CHANNEL_DELETE') return has ? requests.filter((r) => r.id !== d.id) : requests;
  if (event !== 'CHANNEL_CREATE') return requests;
  if (d.is_message_request) {
    const dm = d as DmObject;
    return has ? requests.map((r) => (r.id === dm.id ? dm : r)) : [dm, ...requests];
  }
  return has ? requests.filter((r) => r.id !== d.id) : requests;
}

/** Message requests READY delivered (the reducer keeps them out of the DM list). */
const seedRequests = () => useStore.getState().messageRequestDms as unknown as DmObject[];

/**
 * Keeps the social store current from the gateway for as long as the shell is mounted. A fresh READY
 * (reconnect) reloads the friends list if it was loaded and reseeds the message requests, since events may have been missed.
 */
export function startSocialLive(): () => void {
  useSocial.getState().setMessageRequests(seedRequests());
  return onDispatch((event, data) => {
    if (event === 'READY') {
      const s = useSocial.getState();
      if (s.status === 'ready') void s.loadRelationships();
      s.setMessageRequests(seedRequests());
      return;
    }
    if (
      event.startsWith('RELATIONSHIP_') ||
      event === 'CHANNEL_CREATE' ||
      event === 'CHANNEL_DELETE'
    )
      useSocial.getState().applyEvent(event, data);
  });
}

export const friendsOf = (rows: readonly RelationshipRow[]) =>
  rows.filter((r) => r.type === REL.FRIEND);
export const incomingOf = (rows: readonly RelationshipRow[]) =>
  rows.filter((r) => r.type === REL.INCOMING);
export const outgoingOf = (rows: readonly RelationshipRow[]) =>
  rows.filter((r) => r.type === REL.OUTGOING);

/** Display name for a user object off the wire. */
export const wireName = (u: { username: string; global_name?: string | null }) =>
  u.global_name || u.username;
