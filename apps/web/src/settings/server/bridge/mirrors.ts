// What the whole-server mirror covers, as far as this session knows (BRG-SYN-004): which roles and channels are
// mirrored, for the badges in the role editor and the channel settings. Read from `GET .../bridge/mirrors` once per
// server (and again after a minute), only when a channel of the server is mirrored. Display only: nothing here is an
// authorization input, and a failed read just means no badges.
import { useEffect } from 'react';
import { create } from 'zustand';
import { useGuildBridged } from '../../../lounge/bridge/members';
import { getMirrors, type Mirrors } from './api';

export const MIRRORS_STALE_MS = 60_000;

interface State {
  byGuild: Record<string, Mirrors | null>;
  loadedAt: Record<string, number>;
  load(guildId: string): Promise<void>;
  reset(): void;
}

export const useMirrorStore = create<State>()((set, get) => ({
  byGuild: {},
  loadedAt: {},
  load: async (guildId) => {
    const at = get().loadedAt[guildId];
    if (at !== undefined && Date.now() - at < MIRRORS_STALE_MS) return;
    set((s) => ({ loadedAt: { ...s.loadedAt, [guildId]: Date.now() } }));
    try {
      const m = await getMirrors(guildId);
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: m } }));
    } catch {
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: null } }));
    }
  },
  reset: () => set({ byGuild: {}, loadedAt: {} }),
}));

/** The mirror map of a server while it has a mirrored channel; null otherwise (and no request). */
export function useMirrors(guildId: string | undefined): Mirrors | null {
  const bridged = useGuildBridged(guildId);
  const load = useMirrorStore((s) => s.load);
  useEffect(() => {
    if (guildId && bridged) void load(guildId);
  }, [guildId, bridged, load]);
  const m = useMirrorStore((s) => (guildId ? s.byGuild[guildId] : undefined));
  return bridged ? (m ?? null) : null;
}

/** Whole-server mode only: the role/channel rows are mirrored one by one. */
export function useWholeMirrors(guildId: string | undefined): Mirrors | null {
  const m = useMirrors(guildId);
  return m && m.mode === 'whole_guild' ? m : null;
}

// ---- "Apply on SIDE ROOM": the conflict log hands the Discord-attempted permissions to the role editor

export interface RolePrefill {
  guildId: string;
  roleId: string;
  /** Permission ids (`SEND_MESSAGES`...) Discord's change carried. */
  permissions: string[];
}

interface PrefillState {
  prefill: RolePrefill | null;
  set(p: RolePrefill | null): void;
}

/**
 * The role editor reads this once and clears it. Saving is the clicking person's own edit through the ordinary role
 * checks (`MANAGE_ROLES`, outrank, grant only what they hold): this store applies nothing by itself.
 */
export const useRolePrefill = create<PrefillState>()((set) => ({
  prefill: null,
  set: (prefill) => set({ prefill }),
}));
