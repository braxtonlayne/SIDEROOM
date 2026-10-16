// The Activity the person is in (at most one) and where its window goes (J-15, D-248). The frame is rendered
// once, by `ActivityDock` in the Lounge shell; the channel view that owns the instance only reserves a slot,
// and the dock lays the same iframe element over that slot, or docks it as a small window when the person is
// somewhere else. Reading the layout from this store is what lets the iframe survive navigation.
import { create } from 'zustand';
import type { ActivityInstance } from './types';

export interface SlotRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface Slot {
  channelId: string;
  rect: SlotRect;
}

interface ActivitySession {
  /** The running instance the person has joined. */
  instance: ActivityInstance | null;
  /** The reserved area in the channel view that owns the instance, when that view is on screen. */
  slot: Slot | null;
  /** An instance update arrived: keep it when the person is in it, drop it when they are not (or it ended). */
  adopt(channelId: string, instance: ActivityInstance | null, me: string): void;
  clear(): void;
  setSlot(channelId: string, rect: SlotRect | null): void;
}

export const useActivitySession = create<ActivitySession>()((set, get) => ({
  instance: null,
  slot: null,
  adopt(channelId, instance, me) {
    const cur = get().instance;
    if (instance && instance.participants.some((p) => p.user_id === me)) {
      set({ instance });
    } else if (cur && cur.channel_id === channelId) {
      set({ instance: null });
    }
  },
  clear: () => set({ instance: null }),
  setSlot(channelId, rect) {
    const cur = get().slot;
    if (!rect) {
      if (cur?.channelId === channelId) set({ slot: null });
      return;
    }
    const same =
      cur?.channelId === channelId &&
      cur.rect.left === rect.left &&
      cur.rect.top === rect.top &&
      cur.rect.width === rect.width &&
      cur.rect.height === rect.height;
    if (!same) set({ slot: { channelId, rect } });
  },
}));
