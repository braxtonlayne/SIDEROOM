// Which feature modules are on in a server (GET /guilds/{id}/modules), cached per server. Menus show an entry only
// when its module is on, so a switched-off module costs nothing in the UI.
import { useEffect } from 'react';
import { create } from 'zustand';
import { getModules } from './api';

export const MODULE = {
  scheduled: 'chat.scheduled_messages',
  reminders: 'chat.reminders',
  bookmarks: 'chat.bookmarks',
  pollTemplates: 'chat.poll_templates',
  birthdays: 'community.birthdays',
  leveling: 'community.leveling',
  rss: 'community.rss',
  libraryFeed: 'community.library_feed',
  watchPartyAnnouncer: 'community.watch_party_announcer',
  suggestions: 'community.suggestions',
  tickets: 'community.tickets',
} as const;

/** Whether the server runs a module that has a member-facing tab in the Community dialog. */
export const hasMemberModule = (m: Record<string, boolean> | undefined): boolean =>
  !!m && [MODULE.leveling, MODULE.suggestions, MODULE.tickets, MODULE.birthdays].some((k) => m[k]);

interface ModulesState {
  byGuild: Record<string, Record<string, boolean>>;
  requested: Record<string, true>;
  load(guildId: string): Promise<void>;
  reset(): void;
}

export const useModulesStore = create<ModulesState>()((set, get) => ({
  byGuild: {},
  requested: {},
  load: async (guildId) => {
    if (get().requested[guildId]) return;
    set((s) => ({ requested: { ...s.requested, [guildId]: true } }));
    try {
      const { modules } = await getModules(guildId);
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: modules } }));
    } catch {
      // Not a member, or the server is unreachable: no entries. Asked again after a reload.
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: {} } }));
    }
  },
  reset: () => set({ byGuild: {}, requested: {} }),
}));

/** The modules of a server, loading them on first use. `undefined` until known. */
export function useGuildModules(guildId: string | undefined): Record<string, boolean> | undefined {
  const flags = useModulesStore((s) => (guildId ? s.byGuild[guildId] : undefined));
  const load = useModulesStore((s) => s.load);
  useEffect(() => {
    if (guildId) void load(guildId);
  }, [guildId, load]);
  return flags;
}
