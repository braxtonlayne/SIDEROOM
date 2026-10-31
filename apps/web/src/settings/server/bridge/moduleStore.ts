// Whether the Discord bridge module is on for a server, as far as this session knows (BRG-UI-001, D-381). The
// "Discord bridge" entry of Server Settings reads it so a switched-off module leaves no entry; the Modules screen and
// the screen's own switch-off write to it so the entry appears and goes at once. Filled from the feature list the
// Modules screen loads, or by one request when Server Settings opens for a person who may see the entry.
import { useEffect } from 'react';
import { create } from 'zustand';
import { listFeatures } from '../featuresApi';
import type { Viewer } from '../hooks';
import { BRIDGE_MODULE_ID } from './api';

interface ModuleState {
  /** `true` on and offered, `false` off or switched off by the operator; missing until known. */
  byGuild: Record<string, boolean>;
  requested: Record<string, true>;
  set(guildId: string, on: boolean): void;
  load(guildId: string): Promise<void>;
  reset(): void;
}

export const useBridgeModule = create<ModuleState>()((set, get) => ({
  byGuild: {},
  requested: {},
  set: (guildId, on) => set((s) => ({ byGuild: { ...s.byGuild, [guildId]: on } })),
  load: async (guildId) => {
    if (get().requested[guildId]) return;
    set((s) => ({ requested: { ...s.requested, [guildId]: true } }));
    try {
      const features = await listFeatures(guildId);
      const f = features.find((x) => x.id === BRIDGE_MODULE_ID);
      get().set(guildId, !!f && f.enabled && !f.platform_disabled);
    } catch {
      // Not allowed or unreachable: no entry. The Modules screen corrects it when it loads.
      get().set(guildId, false);
    }
  },
  reset: () => set({ byGuild: {}, requested: {} }),
}));

/**
 * The bridge entry of Server Settings: the module is on and the viewer is the owner or holds MANAGE_GUILD.
 * `pending` is true while that is still being read, so a deep link to the screen waits instead of being sent
 * to the first section.
 */
export function useBridgeNav(
  guildId: string,
  viewer: Viewer,
): { visible: boolean; pending: boolean } {
  const mayManage = viewer.ready && (viewer.isOwner || viewer.can('MANAGE_GUILD'));
  const on = useBridgeModule((s) => s.byGuild[guildId]);
  const load = useBridgeModule((s) => s.load);
  useEffect(() => {
    if (mayManage) void load(guildId);
  }, [mayManage, guildId, load]);
  return { visible: mayManage && on === true, pending: mayManage && on === undefined };
}
