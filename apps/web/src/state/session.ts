import { create } from 'zustand';

const KEY = 'sr.lastSection.v1';
const DEFAULT_SECTION = '/channels';

/** The sections `/app` can reopen (FU-ENT-4): the Lounge, Cinema, the Study, the Arcade, the Library and Music. */
const SECTION =
  /^\/(?:channels(?:\/[^?#\s]*)?|cinema(?:\/[^?#\s]*)?|study(?:\/[^?#\s]*)?|games(?:\/emulator)?|library(?:\/[^?#\s]*)?|music(?:\/[^?#\s]*)?)$/;

/** True for a path worth reopening: a section page, never settings, auth or error pages, never a query string. */
export const isSectionPath = (pathname: string): boolean => SECTION.test(pathname);

function readStored(): string {
  try {
    const v = localStorage.getItem(KEY);
    if (v && isSectionPath(v)) return v;
  } catch {
    /* storage unavailable: the default */
  }
  return DEFAULT_SECTION;
}

interface UiState {
  /** Last section visited; used to redirect from / and /app when logged in. */
  lastSection: string;
  setLastSection: (section: string) => void;
}

export const useUiStore = create<UiState>((set) => ({
  lastSection: readStored(),
  setLastSection: (lastSection) => {
    set({ lastSection });
    try {
      localStorage.setItem(KEY, lastSection);
    } catch {
      /* storage unavailable: kept for this tab only */
    }
  },
}));

/**
 * Forgets the remembered section (sign-out): the next account on this browser starts at the default instead of
 * reopening the previous member's page (FU-10).
 */
export function forgetSection(): void {
  useUiStore.setState({ lastSection: DEFAULT_SECTION });
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* storage unavailable: nothing was kept */
  }
}

/** Remembers `pathname` as the last section when it is one (a no-op for every other page). */
export function rememberSection(pathname: string): void {
  if (isSectionPath(pathname) && useUiStore.getState().lastSection !== pathname) {
    useUiStore.getState().setLastSection(pathname);
  }
}
