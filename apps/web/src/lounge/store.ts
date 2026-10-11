/**
 * Lounge UI state that is not server data: remembered locations, collapsed categories, panel sizes,
 * recent quick-switcher picks, the (placeholder) voice connection. Persisted to localStorage where available.
 */
import { create } from 'zustand';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';

const safeStorage: StateStorage = {
  getItem: (k) => {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  },
  setItem: (k, v) => {
    try {
      localStorage.setItem(k, v);
    } catch {
      /* storage unavailable (private mode, quota): state simply is not persisted */
    }
  },
  removeItem: (k) => {
    try {
      localStorage.removeItem(k);
    } catch {
      /* ignore */
    }
  },
};

export const RIGHT_PANEL_MIN = 240;
export const RIGHT_PANEL_MAX = 340;

export interface VoiceConnection {
  guildId: string;
  channelId: string;
  name: string;
}

export interface LoungeUiState {
  /** Where to land on `/channels` after a reload (LNG-SHL-005). */
  lastLocation: { guildId: string; channelId?: string } | null;
  lastChannelByGuild: Record<string, string>;
  collapsedCategories: Record<string, boolean>;
  openFolders: Record<string, boolean>;
  showSectionBar: boolean;
  memberListOpen: boolean;
  hideMutedChannels: Record<string, boolean>;
  rightPanelWidth: number;
  recentSwitcher: string[];
  // Not persisted:
  voice: VoiceConnection | null;
  micMuted: boolean;
  deafened: boolean;

  setLocation(guildId: string, channelId?: string): void;
  toggleCategory(id: string): void;
  toggleFolder(id: string): void;
  setShowSectionBar(v: boolean): void;
  setMemberListOpen(v: boolean): void;
  toggleHideMuted(guildId: string): void;
  setRightPanelWidth(w: number): void;
  pushRecent(key: string): void;
  joinVoice(v: VoiceConnection | null): void;
  toggleMic(): void;
  toggleDeafen(): void;
}

export const clampPanel = (w: number) =>
  Math.min(RIGHT_PANEL_MAX, Math.max(RIGHT_PANEL_MIN, Math.round(w)));

export const useLoungeUi = create<LoungeUiState>()(
  persist(
    (set) => ({
      lastLocation: null,
      lastChannelByGuild: {},
      collapsedCategories: {},
      openFolders: {},
      showSectionBar: true,
      memberListOpen: true,
      hideMutedChannels: {},
      rightPanelWidth: 240,
      recentSwitcher: [],
      voice: null,
      micMuted: false,
      deafened: false,

      setLocation: (guildId, channelId) =>
        set((s) => ({
          lastLocation: channelId ? { guildId, channelId } : { guildId },
          lastChannelByGuild: channelId
            ? { ...s.lastChannelByGuild, [guildId]: channelId }
            : s.lastChannelByGuild,
        })),
      toggleCategory: (id) =>
        set((s) => ({
          collapsedCategories: { ...s.collapsedCategories, [id]: !s.collapsedCategories[id] },
        })),
      toggleFolder: (id) =>
        set((s) => ({ openFolders: { ...s.openFolders, [id]: !s.openFolders[id] } })),
      setShowSectionBar: (showSectionBar) => set({ showSectionBar }),
      setMemberListOpen: (memberListOpen) => set({ memberListOpen }),
      toggleHideMuted: (guildId) =>
        set((s) => ({
          hideMutedChannels: { ...s.hideMutedChannels, [guildId]: !s.hideMutedChannels[guildId] },
        })),
      setRightPanelWidth: (w) => set({ rightPanelWidth: clampPanel(w) }),
      pushRecent: (key) =>
        set((s) => ({
          recentSwitcher: [key, ...s.recentSwitcher.filter((k) => k !== key)].slice(0, 8),
        })),
      joinVoice: (voice) => set({ voice }),
      toggleMic: () => set((s) => ({ micMuted: !s.micMuted })),
      toggleDeafen: () =>
        set((s) => ({ deafened: !s.deafened, micMuted: s.deafened ? s.micMuted : true })),
    }),
    {
      name: 'sr.lounge.ui',
      storage: createJSONStorage(() => safeStorage),
      partialize: (s) => ({
        lastLocation: s.lastLocation,
        lastChannelByGuild: s.lastChannelByGuild,
        collapsedCategories: s.collapsedCategories,
        openFolders: s.openFolders,
        showSectionBar: s.showSectionBar,
        memberListOpen: s.memberListOpen,
        hideMutedChannels: s.hideMutedChannels,
        rightPanelWidth: s.rightPanelWidth,
        recentSwitcher: s.recentSwitcher,
      }),
    },
  ),
);
