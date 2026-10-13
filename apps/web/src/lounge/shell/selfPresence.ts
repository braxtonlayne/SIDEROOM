/**
 * The person's own presence choice (online / idle / dnd / invisible plus a custom status). The server keeps it
 * (`/users/@me/presence`); this store mirrors it so the user panel can show "Invisible", which other people
 * (and the gateway store) only ever see as offline.
 */
import { create } from 'zustand';
import { useStore } from '../../store';
import { getPresence, patchPresence, type PresenceBody, type PresenceObject } from './api';

export type SelfStatus = 'online' | 'idle' | 'dnd' | 'invisible';

export interface SelfPresenceState {
  status: SelfStatus;
  customText: string | null;
  loaded: boolean;
  load(): Promise<void>;
  update(body: PresenceBody): Promise<void>;
}

const asStatus = (s: string): SelfStatus =>
  s === 'idle' || s === 'dnd' || s === 'invisible' ? s : 'online';

function mirrorIntoStore(userId: string | null, p: PresenceObject) {
  if (!userId) return;
  const status = p.status === 'invisible' ? 'offline' : p.status;
  const text = p.custom_status?.text;
  useStore.getState().applyDispatch('PRESENCE_UPDATE', {
    user: { id: userId },
    status,
    activities: text ? [{ type: 4, name: 'Custom Status', state: text }] : [],
  });
}

export const useSelfPresence = create<SelfPresenceState>()((set) => ({
  status: 'online',
  customText: null,
  loaded: false,
  async load() {
    try {
      const p = await getPresence();
      set({ status: asStatus(p.status), customText: p.custom_status?.text ?? null, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },
  async update(body) {
    const p = await patchPresence(body);
    set({ status: asStatus(p.status), customText: p.custom_status?.text ?? null, loaded: true });
    mirrorIntoStore(useStore.getState().currentUserId, p);
  },
}));
