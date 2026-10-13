/**
 * Lounge UI state that is not server data: remembered locations, collapsed categories, panel sizes,
 * recent quick-switcher picks, the (placeholder) voice connection. Persisted to localStorage where available.
 */
import { create } from 'zustand';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';
import type { RailEntry } from './data';

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

/** Modal dialogs opened from menus anywhere in the shell; rendered once by LoungeDialogs. */
export type LoungeDialog =
  | { kind: 'addServer' }
  | { kind: 'createChannel'; guildId: string; parentId?: string | undefined; category?: boolean }
  | { kind: 'invite'; guildId: string; channelId?: string | undefined }
  | { kind: 'leaveServer'; guildId: string }
  | { kind: 'folder'; folderId: string }
  | { kind: 'createDm' }
  | { kind: 'customStatus' }
  | { kind: 'editChannel'; guildId: string; channelId: string }
  | { kind: 'deleteChannel'; guildId: string; channelId: string }
  | { kind: 'notifications'; guildId: string; channelId?: string | undefined };

export interface LoungeUiState {
  /** Where to land on `/channels` after a reload (LNG-SHL-005). */
  lastLocation: { guildId: string; channelId?: string } | null;
  lastChannelByGuild: Record<string, string>;
  collapsedCategories: Record<string, boolean>;
  openFolders: Record<string, boolean>;
  /** Server rail order and folders chosen by the person (device-local); null = server order. */
  railLayout: RailEntry[] | null;
  showSectionBar: boolean;
  memberListOpen: boolean;
  hideMutedChannels: Record<string, boolean>;
  rightPanelWidth: number;
  recentSwitcher: string[];
  /** Info banners the person dismissed, by id. */
  dismissedBanners: Record<string, boolean>;
  // Not persisted:
  /** What Home shows when no conversation is open. */
  homeView: 'friends' | 'requests';
  voice: VoiceConnection | null;
  micMuted: boolean;
  deafened: boolean;
  dialog: LoungeDialog | null;

  dismissBanner(id: string): void;
  setHomeView(v: 'friends' | 'requests'): void;
  openDialog(d: LoungeDialog): void;
  closeDialog(): void;
  setLocation(guildId: string, channelId?: string): void;
  toggleCategory(id: string): void;
  toggleFolder(id: string): void;
  setRailLayout(layout: RailEntry[] | null): void;
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
      railLayout: null,
      showSectionBar: true,
      memberListOpen: true,
      hideMutedChannels: {},
      rightPanelWidth: 240,
      recentSwitcher: [],
      dismissedBanners: {},
      homeView: 'friends',
      voice: null,
      micMuted: false,
      deafened: false,
      dialog: null,

      dismissBanner: (id) =>
        set((s) => ({ dismissedBanners: { ...s.dismissedBanners, [id]: true } })),
      setHomeView: (homeView) => set({ homeView }),
      openDialog: (dialog) => set({ dialog }),
      closeDialog: () => set({ dialog: null }),
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
      setRailLayout: (railLayout) => set({ railLayout }),
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
        railLayout: s.railLayout,
        showSectionBar: s.showSectionBar,
        memberListOpen: s.memberListOpen,
        hideMutedChannels: s.hideMutedChannels,
        rightPanelWidth: s.rightPanelWidth,
        recentSwitcher: s.recentSwitcher,
        dismissedBanners: s.dismissedBanners,
      }),
    },
  ),
);
