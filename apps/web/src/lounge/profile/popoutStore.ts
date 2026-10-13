/**
 * Popout host state (LNG-SHL-012). Profile popouts open next to whatever was clicked (an avatar, a name, a
 * member row, a mention); only one popout is open at a time. The "View Full Profile" modal is separate.
 */
import { create } from 'zustand';

export interface ProfileTarget {
  userId: string;
  /** Server whose roles and nickname apply; undefined in DMs. */
  guildId?: string | undefined;
}

export interface PopoutState {
  profile: (ProfileTarget & { anchor: HTMLElement }) | null;
  /** The full profile modal. */
  full: ProfileTarget | null;
  openProfile(target: ProfileTarget, anchor: HTMLElement): void;
  closeProfile(): void;
  openFull(target: ProfileTarget): void;
  closeFull(): void;
}

export const usePopout = create<PopoutState>()((set) => ({
  profile: null,
  full: null,
  // Opening the same user again from the same place closes the popout, like a toggle.
  openProfile: (target, anchor) =>
    set((s) =>
      s.profile?.userId === target.userId && s.profile.anchor === anchor
        ? { profile: null }
        : { profile: { ...target, anchor } },
    ),
  closeProfile: () => set({ profile: null }),
  openFull: (target) => set({ full: target, profile: null }),
  closeFull: () => set({ full: null }),
}));

/** Opens the profile popout for a person; `anchor` is the element that was clicked. */
export function openProfilePopout(target: ProfileTarget, anchor: HTMLElement | null) {
  if (anchor) usePopout.getState().openProfile(target, anchor);
  else usePopout.getState().openFull(target);
}
