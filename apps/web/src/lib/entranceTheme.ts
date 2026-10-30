// The Entrance theme for signed-out pages (ADM-PLT-004, D-254), with nothing of the app's preferences model.
// A public surface knows only two things: this browser's saved theme choice (read from the same storage entry
// the app writes, never written here) and the platform's default for the entrance, from the public
// `GET /sr/appearance/defaults`. Resolution order: the viewer's own entrance choice, their theme for
// everything, the platform default, the shipped default (`resolveTheme` in `@sideroom/ui`).
import { isThemeId, type Section, type ThemeId } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../api/client';

export type EntranceThemes = Partial<Record<Section, ThemeId>>;

const PREFS_KEY = 'sr.user.prefs';
/** The shipped theme everyone starts with; a saved theme other than this is a deliberate global choice. */
const SHIPPED_THEME = 'lounge-dark';

export interface SavedTheme {
  /** The viewer's per-section choices. */
  own: EntranceThemes;
  /** True when the viewer chose a theme for everything. */
  hasGlobalChoice: boolean;
}

/** Reads the saved choice defensively: stale or hand-edited storage can never break rendering. */
export function readSavedTheme(): SavedTheme {
  try {
    const raw = JSON.parse(localStorage.getItem(PREFS_KEY) ?? 'null') as {
      state?: { prefs?: { theme?: unknown; sectionThemes?: Record<string, unknown> } };
    } | null;
    const prefs = raw?.state?.prefs;
    const entrance = prefs?.sectionThemes?.entrance;
    const theme = prefs?.theme;
    return {
      own: isThemeId(entrance) ? { entrance } : {},
      hasGlobalChoice: typeof theme === 'string' && theme !== SHIPPED_THEME,
    };
  } catch {
    return { own: {}, hasGlobalChoice: false };
  }
}

/** Keep only the entrance's default and a known theme id. */
export function parseEntranceDefault(raw: unknown): EntranceThemes {
  const map =
    raw && typeof raw === 'object'
      ? (raw as { default_theme_by_section?: unknown }).default_theme_by_section
      : undefined;
  const value =
    map && typeof map === 'object' ? (map as Record<string, unknown>).entrance : undefined;
  return isThemeId(value) ? { entrance: value } : {};
}

let platform: EntranceThemes = {};
let requested = false;
const listeners = new Set<() => void>();

function loadPlatform() {
  if (requested) return;
  requested = true;
  apiFetch<unknown>('/sr/appearance/defaults', { retries: 0 })
    .then((res) => {
      platform = parseEntranceDefault(res);
      for (const l of listeners) l();
    })
    .catch(() => {
      // No default known: the entrance keeps its shipped theme.
    });
}

/** The per-section map to hand a `ThemeProvider`. */
export function useEntranceThemes(): EntranceThemes {
  const [, bump] = useState(0);
  useEffect(() => {
    const l = () => bump((n) => n + 1);
    listeners.add(l);
    loadPlatform();
    return () => void listeners.delete(l);
  }, []);
  const saved = readSavedTheme();
  return saved.hasGlobalChoice ? saved.own : { ...platform, ...saved.own };
}
