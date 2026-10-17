// The platform's default theme per section (ADM-PLT-004, D-254), chosen in Admin and read from
// `GET /sr/appearance/defaults` (public: the entrance is themed before sign-in). Resolution order for a
// section: the viewer's own per-section choice, their own theme for everything, this default, the shipped
// default (`resolveTheme` in `@sideroom/ui` ends at the shipped one).
import { isThemeId, type Section, type ThemeId } from '@sideroom/ui';
import { useEffect } from 'react';
import { create } from 'zustand';
import { apiFetch } from '../../api/client';

export type PlatformThemes = Partial<Record<Section, ThemeId>>;

/** The server names the Live TV guide `tv`; the theme runtime names it `live-tv`. */
const SECTION_OF: Record<string, Section> = {
  lounge: 'lounge',
  entrance: 'entrance',
  cinema: 'cinema',
  library: 'library',
  support: 'support',
  tv: 'live-tv',
};

/** Keep only known sections and theme ids; stale or hand-edited data can never break rendering. */
export function parsePlatformThemes(raw: unknown): PlatformThemes {
  const out: PlatformThemes = {};
  const map =
    raw && typeof raw === 'object'
      ? (raw as { default_theme_by_section?: unknown }).default_theme_by_section
      : undefined;
  if (!map || typeof map !== 'object') return out;
  for (const [key, value] of Object.entries(map)) {
    const section = SECTION_OF[key];
    if (section && isThemeId(value)) out[section] = value;
  }
  return out;
}

interface PlatformThemeState {
  themes: PlatformThemes;
  loaded: boolean;
  load(): Promise<void>;
}

export const usePlatformThemes = create<PlatformThemeState>((set, get) => ({
  themes: {},
  loaded: false,
  load: async () => {
    if (get().loaded) return;
    set({ loaded: true });
    try {
      const res = await apiFetch<unknown>('/sr/appearance/defaults', { retries: 0 });
      set({ themes: parsePlatformThemes(res) });
    } catch {
      // No defaults known: every section keeps its shipped theme.
    }
  },
}));

/** The platform defaults, fetched once per page load. */
export function usePlatformThemeDefaults(): PlatformThemes {
  const themes = usePlatformThemes((s) => s.themes);
  const load = usePlatformThemes((s) => s.load);
  useEffect(() => {
    void load();
  }, [load]);
  return themes;
}

/**
 * The per-section map to hand a ThemeProvider: the viewer's own per-section choices win; a viewer who chose
 * a theme for everything keeps it (their global choice outranks the platform default); everyone else gets
 * the platform default of each section.
 */
export function withPlatformDefaults(
  own: PlatformThemes,
  hasGlobalChoice: boolean,
  platform: PlatformThemes,
): PlatformThemes {
  return hasGlobalChoice ? own : { ...platform, ...own };
}
