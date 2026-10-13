/**
 * Ignored people (`/users/@me/ignores`, Q110). Blocked people come from the relationship list; together they
 * decide which authors collapse in the message list (LNG-MSGUI-032).
 */
import { create } from 'zustand';
import { apiFetch } from '../../api/client';

export interface IgnoreState {
  ids: string[];
  status: 'idle' | 'loading' | 'ready' | 'error';
  load(): Promise<void>;
  add(userId: string): void;
  remove(userId: string): void;
  reset(): void;
}

export const useIgnores = create<IgnoreState>()((set, get) => ({
  ids: [],
  status: 'idle',
  async load() {
    if (get().status === 'loading') return;
    set({ status: 'loading' });
    try {
      const list = await apiFetch<{ id: string }[]>('/users/@me/ignores', { retries: 0 });
      set({ ids: list.map((u) => u.id), status: 'ready' });
    } catch {
      set({ status: 'error' });
    }
  },
  add: (userId) => set((s) => (s.ids.includes(userId) ? s : { ids: [...s.ids, userId] })),
  remove: (userId) => set((s) => ({ ids: s.ids.filter((i) => i !== userId) })),
  reset: () => set({ ids: [], status: 'idle' }),
}));
