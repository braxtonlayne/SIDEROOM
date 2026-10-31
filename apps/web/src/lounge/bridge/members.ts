// The Discord side of a bridged server as members (owner direction: Discord should feel like part of SIDE ROOM):
// listed in the member sidebar, shown in a popout, mentionable and shown typing. Asked once per server (and again
// after a minute) only when one of its channels is mapped, and only after sign-in (D-385).
import { useEffect } from 'react';
import { create } from 'zustand';
import { listBridgedMembers, type BridgedMember } from '../../settings/server/bridge/api';
import { useStore } from '../../store';

export const BRIDGED_STALE_MS = 60_000;
const NONE: readonly BridgedMember[] = [];

interface State {
  byGuild: Record<string, readonly BridgedMember[]>;
  loadedAt: Record<string, number>;
  load(guildId: string): Promise<void>;
  reset(): void;
}

export const useBridgedMembers = create<State>()((set, get) => ({
  byGuild: {},
  loadedAt: {},
  load: async (guildId) => {
    const at = get().loadedAt[guildId];
    if (at !== undefined && Date.now() - at < BRIDGED_STALE_MS) return;
    set((s) => ({ loadedAt: { ...s.loadedAt, [guildId]: Date.now() } }));
    try {
      const list = await listBridgedMembers(guildId);
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: list } }));
    } catch {
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: NONE } }));
    }
  },
  reset: () => set({ byGuild: {}, loadedAt: {} }),
}));

/** Does the server have a channel the bridge mirrors (the client already has `sr_bridge` on the channel)? */
export function useGuildBridged(guildId: string | undefined): boolean {
  return useStore((s) => {
    if (!guildId) return false;
    for (const c of Object.values(s.channels))
      if (c.guild_id === guildId && c.sr_bridge) return true;
    return false;
  });
}

/** The Discord members of a server; empty (and no request) while none of its channels is mapped. */
export function useGuildBridgedMembers(guildId: string | undefined): readonly BridgedMember[] {
  const bridged = useGuildBridged(guildId);
  const load = useBridgedMembers((s) => s.load);
  useEffect(() => {
    if (guildId && bridged) void load(guildId);
  }, [guildId, bridged, load]);
  const list = useBridgedMembers((s) => (guildId ? s.byGuild[guildId] : undefined));
  return bridged ? (list ?? NONE) : NONE;
}

/** One Discord member by id, across the servers loaded so far. */
export function findBridgedMember(id: string): BridgedMember | undefined {
  for (const list of Object.values(useBridgedMembers.getState().byGuild)) {
    const m = list.find((x) => x.id === id);
    if (m) return m;
  }
  return undefined;
}

export function useBridgedMember(id: string | undefined): BridgedMember | undefined {
  return useBridgedMembers((s) => {
    if (!id) return undefined;
    for (const list of Object.values(s.byGuild)) {
      const m = list.find((x) => x.id === id);
      if (m) return m;
    }
    return undefined;
  });
}

/** Same-origin proxied avatar only (D-176). */
export const bridgedAvatar = (m: Pick<BridgedMember, 'avatar'>): string | null =>
  m.avatar?.startsWith('/cdn/external/') ? m.avatar : null;

export const bridgedColor = (m: Pick<BridgedMember, 'role_color'>): string | undefined =>
  m.role_color ? `#${m.role_color.toString(16).padStart(6, '0')}` : undefined;
