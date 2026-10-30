// The scrapbook module's status per server (GET /guilds/{id}/scrapbook/status), cached. Menus and banners show
// only when the operator offered the module AND this server switched it on, so a hidden module costs one
// small request per server and nothing else.
import { useEffect } from 'react';
import { create } from 'zustand';
import { getStatus, type ScrapbookStatus } from './api';

interface StatusState {
  byGuild: Record<string, ScrapbookStatus | null>;
  requested: Record<string, true>;
  load(guildId: string): Promise<void>;
  /** Forget a server's status, e.g. after the shelf changed. */
  refresh(guildId: string): Promise<void>;
  reset(): void;
}

export const useScrapbookStatus = create<StatusState>()((set, get) => ({
  byGuild: {},
  requested: {},
  load: async (guildId) => {
    if (get().requested[guildId]) return;
    set((s) => ({ requested: { ...s.requested, [guildId]: true } }));
    await get().refresh(guildId);
  },
  refresh: async (guildId) => {
    try {
      const status = await getStatus(guildId);
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: status } }));
    } catch {
      // Not a member, or unreachable: the module is simply not there. Asked again after a reload.
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: null } }));
    }
  },
  reset: () => set({ byGuild: {}, requested: {} }),
}));

/** The status of a server's scrapbook, loading it on first use. `undefined` until known, `null` when unavailable. */
export function useGuildScrapbook(guildId: string | undefined): ScrapbookStatus | null | undefined {
  const status = useScrapbookStatus((s) => (guildId ? s.byGuild[guildId] : undefined));
  const load = useScrapbookStatus((s) => s.load);
  useEffect(() => {
    if (guildId && guildId !== '@me') void load(guildId);
  }, [guildId, load]);
  return status;
}

/** Whether the scrapbook is on for this server (offered by the operator and switched on here). */
export const scrapbookOn = (s: ScrapbookStatus | null | undefined): s is ScrapbookStatus =>
  !!s && s.offered && s.enabled;
