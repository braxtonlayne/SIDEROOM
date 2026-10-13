/**
 * Local user preferences (appearance, notification defaults, advanced). Persisted to localStorage only:
 * the server has no user-settings blob yet (LNG-USET-002 is unbuilt), so nothing here syncs across devices.
 * Applied through the ThemeProvider runtime variables (docs/10 section 6).
 */
import type { AppearanceSettings, ThemeId } from '@sideroom/ui';
import { useMediaQuery } from '@sideroom/ui';
import { useMemo, type CSSProperties } from 'react';
import { create } from 'zustand';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';

export const THEME_CHOICES = ['lounge-dark', 'lounge-light', 'lounge-ash', 'lounge-onyx'] as const;
export type ThemeChoice = 'system' | (typeof THEME_CHOICES)[number];
export type MessageDisplay = 'cozy' | 'compact';
export type DefaultNotify = 'all' | 'mentions' | 'nothing';

export interface UserPrefs {
  theme: ThemeChoice;
  messageDisplay: MessageDisplay;
  /** Percent of the 16px base font, 75..150. */
  fontScale: number;
  /** 0..100. */
  saturation: number;
  reducedMotion: AppearanceSettings['reducedMotion'];
  /** Interface zoom percent, 50..200. */
  zoom: number;
  defaultNotify: DefaultNotify;
  desktopNotifications: boolean;
  soundMessage: boolean;
  soundMention: boolean;
  soundCall: boolean;
  developerMode: boolean;
}

export const DEFAULT_PREFS: UserPrefs = {
  // The Lounge has always been dark by default; "Sync with computer" is opt-in.
  theme: 'lounge-dark',
  messageDisplay: 'cozy',
  fontScale: 100,
  saturation: 100,
  reducedMotion: 'system',
  zoom: 100,
  defaultNotify: 'mentions',
  desktopNotifications: false,
  soundMessage: true,
  soundMention: true,
  soundCall: true,
  developerMode: false,
};

export const FONT_SCALE_RANGE = [75, 150] as const;
export const ZOOM_RANGE = [50, 200] as const;

const clamp = (n: unknown, lo: number, hi: number, fallback: number) =>
  typeof n === 'number' && Number.isFinite(n)
    ? Math.min(hi, Math.max(lo, Math.round(n)))
    : fallback;
const oneOf = <T extends string>(v: unknown, allowed: readonly T[], fallback: T): T =>
  typeof v === 'string' && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
const bool = (v: unknown, fallback: boolean) => (typeof v === 'boolean' ? v : fallback);

/** Stale or hand-edited storage can never break rendering: every field is validated. */
export function sanitizePrefs(raw: unknown): UserPrefs {
  const r = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_PREFS;
  return {
    theme: oneOf(r.theme, ['system', ...THEME_CHOICES] as const, d.theme),
    messageDisplay: oneOf(r.messageDisplay, ['cozy', 'compact'] as const, d.messageDisplay),
    fontScale: clamp(r.fontScale, ...FONT_SCALE_RANGE, d.fontScale),
    saturation: clamp(r.saturation, 0, 100, d.saturation),
    reducedMotion: oneOf(r.reducedMotion, ['system', 'on', 'off'] as const, d.reducedMotion),
    zoom: clamp(r.zoom, ...ZOOM_RANGE, d.zoom),
    defaultNotify: oneOf(r.defaultNotify, ['all', 'mentions', 'nothing'] as const, d.defaultNotify),
    desktopNotifications: bool(r.desktopNotifications, d.desktopNotifications),
    soundMessage: bool(r.soundMessage, d.soundMessage),
    soundMention: bool(r.soundMention, d.soundMention),
    soundCall: bool(r.soundCall, d.soundCall),
    developerMode: bool(r.developerMode, d.developerMode),
  };
}

export const prefsEqual = (a: UserPrefs, b: UserPrefs) =>
  (Object.keys(DEFAULT_PREFS) as (keyof UserPrefs)[]).every((k) => a[k] === b[k]);

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
      /* storage unavailable: preferences simply are not persisted */
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

interface PrefsState {
  prefs: UserPrefs;
  save(next: UserPrefs): void;
}

export const usePrefsStore = create<PrefsState>()(
  persist(
    (set) => ({
      prefs: DEFAULT_PREFS,
      save: (next) => set({ prefs: sanitizePrefs(next) }),
    }),
    {
      name: 'sr.user.prefs',
      version: 1,
      storage: createJSONStorage(() => safeStorage),
      partialize: (s) => ({ prefs: s.prefs }),
      merge: (persisted, current) => ({
        ...current,
        prefs: sanitizePrefs((persisted as { prefs?: unknown } | undefined)?.prefs),
      }),
    },
  ),
);

export const useSavedPrefs = () => usePrefsStore((s) => s.prefs);

export interface AppliedTheme {
  userGlobal: ThemeId;
  appearance: Partial<AppearanceSettings>;
  style: CSSProperties | undefined;
  messageDisplay: MessageDisplay;
}

/** What to hand a ThemeProvider for a given set of prefs. `systemDark` resolves "Sync with computer". */
export function applyPrefs(p: UserPrefs, systemDark: boolean): AppliedTheme {
  const userGlobal: ThemeId =
    p.theme === 'system' ? (systemDark ? 'lounge-dark' : 'lounge-light') : p.theme;
  return {
    userGlobal,
    appearance: {
      fontSize: (16 * p.fontScale) / 100,
      saturation: p.saturation,
      reducedMotion: p.reducedMotion,
    },
    // CSS zoom scales layout and text together; it is a no-op at 100.
    style: p.zoom === 100 ? undefined : ({ zoom: p.zoom / 100 } as CSSProperties),
    messageDisplay: p.messageDisplay,
  };
}

/** ThemeProvider props for the saved preferences (used by the Lounge shell). */
export function useAppliedTheme(prefs?: UserPrefs): AppliedTheme {
  const saved = useSavedPrefs();
  const dark = useMediaQuery('(prefers-color-scheme: dark)');
  const p = prefs ?? saved;
  return useMemo(() => applyPrefs(p, dark), [p, dark]);
}
