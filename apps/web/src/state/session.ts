import { create } from 'zustand';

interface UiState {
  /** Last section visited; used to redirect from / and /login when logged in. */
  lastSection: string;
  setLastSection: (section: string) => void;
}

export const useUiStore = create<UiState>((set) => ({
  lastSection: '/channels',
  setLastSection: (lastSection) => set({ lastSection }),
}));
