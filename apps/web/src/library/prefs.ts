// Library preferences: the synced `library.hide_age_restricted` setting (D-184) and the reader's local prefs.
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { create } from 'zustand';
import { apiFetch } from '../api/client';
import type { AgeFilter, ReadingDirection } from './types';

/* ------------------------------------------------ synced settings blob */

interface SettingsResponse {
  version: number;
  settings: Record<string, unknown>;
}

export const SETTINGS_KEY = ['library', 'settings-blob'] as const;

/** Reads `library.hide_age_restricted` out of the settings blob (default false). */
export function hideRestrictedFrom(settings: Record<string, unknown> | undefined): boolean {
  const lib = settings?.library;
  if (lib && typeof lib === 'object' && 'hide_age_restricted' in lib)
    return (lib as { hide_age_restricted?: unknown }).hide_age_restricted === true;
  return false;
}

/**
 * `{ hideRestricted }` from `GET /users/@me/settings`. The server already applies it as the default `age`
 * filter; the UI reads it to show that default in the filter control and to keep restricted titles out of
 * client-built rows. While the blob loads (or fails) the answer is false: restricted titles are shown (D-184).
 */
export function useLibrarySettings() {
  const q = useQuery({
    queryKey: SETTINGS_KEY,
    queryFn: () => apiFetch<SettingsResponse>('/users/@me/settings', { retries: 0 }),
    staleTime: 60_000,
  });
  return { hideRestricted: hideRestrictedFrom(q.data?.settings), loaded: q.isSuccess };
}

/** Writes the setting as a merge patch and refreshes the cache. */
export function useSetHideRestricted() {
  const qc = useQueryClient();
  return async (hide: boolean) => {
    const res = await apiFetch<SettingsResponse>('/users/@me/settings', {
      method: 'PATCH',
      json: { settings: { library: { hide_age_restricted: hide } } },
      retries: 0,
    });
    qc.setQueryData(SETTINGS_KEY, res);
    void qc.invalidateQueries({ queryKey: ['library', 'browse'] });
  };
}

/** The `age` filter a listing starts with. */
export const defaultAgeFilter = (hideRestricted: boolean): AgeFilter =>
  hideRestricted ? 'hide' : 'include';

/* ------------------------------------------------------ reader prefs */

export type ReaderMode = 'paged' | 'double' | 'strip';
export type FitMode = 'width' | 'height' | 'original' | 'smart';
export type ReaderBg = 'black' | 'grey' | 'white' | 'sepia';

export interface ReaderPrefs {
  mode: ReaderMode;
  /** Page direction for paged and double modes. */
  direction: 'rtl' | 'ltr';
  fit: FitMode;
  background: ReaderBg;
  /** Gap between strip pages, px. 0 is gapless. */
  gap: number;
  smoothing: boolean;
  /** 30..100; below 100 dims the page with an overlay. */
  brightness: number;
  tapZones: boolean;
  /** Double page: show the first page alone (cover). */
  coverOffset: boolean;
  /** Cut the borders the ingest found (pages without crop data show whole). */
  cropBorders: boolean;
  /** Keep the current and next chapter in the browser for offline reading (LIB-RDR-015). */
  offline: boolean;
  /** How many pages after the current one are fetched ahead, 1..10 (LNG-USET-035). */
  preloadCount: number;
}

export const PRELOAD_RANGE = [1, 10] as const;
/** Stored values can be anything: a whole number inside the range, else the default. */
export function preloadCountOf(v: unknown): number {
  return typeof v === 'number' && Number.isFinite(v)
    ? Math.min(PRELOAD_RANGE[1], Math.max(PRELOAD_RANGE[0], Math.round(v)))
    : DEFAULT_READER_PREFS.preloadCount;
}

export const DEFAULT_READER_PREFS: ReaderPrefs = {
  mode: 'paged',
  direction: 'rtl',
  fit: 'smart',
  background: 'black',
  gap: 0,
  smoothing: true,
  brightness: 100,
  tapZones: true,
  coverOffset: true,
  cropBorders: false,
  offline: true,
  preloadCount: 3,
};

const STORE_KEY = 'sr.library.reader.v1';

interface Persisted {
  global: Partial<ReaderPrefs>;
  series: Record<string, Partial<ReaderPrefs>>;
}

function load(): Persisted {
  try {
    const raw = localStorage.getItem(STORE_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<Persisted>;
      return { global: p.global ?? {}, series: p.series ?? {} };
    }
  } catch {
    /* ignore */
  }
  return { global: {}, series: {} };
}
function persist(p: Persisted) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(p));
  } catch {
    /* best effort */
  }
}

interface ReaderPrefsState extends Persisted {
  /** Change prefs: for this series only (override) or as the new default for every series. */
  set(patch: Partial<ReaderPrefs>, scope: { seriesId: string; override: boolean }): void;
  clearOverride(seriesId: string): void;
}

export const useReaderPrefsStore = create<ReaderPrefsState>((set, get) => ({
  ...load(),
  set(patch, { seriesId, override }) {
    const s = get();
    const next: Persisted = override
      ? {
          global: s.global,
          series: { ...s.series, [seriesId]: { ...s.series[seriesId], ...patch } },
        }
      : { global: { ...s.global, ...patch }, series: s.series };
    persist(next);
    set(next);
  },
  clearOverride(seriesId) {
    const s = get();
    const series = { ...s.series };
    delete series[seriesId];
    persist({ global: s.global, series });
    set({ series });
  },
}));

/**
 * Effective prefs for a series: defaults, then the global choice, then the series override. The direction
 * default comes from the series (`rtl`, `ltr` or `vertical`, which implies the strip) until the member picks.
 */
export function resolveReaderPrefs(
  s: Persisted,
  seriesId: string,
  seriesDirection: ReadingDirection,
): ReaderPrefs {
  const fromSeries: Partial<ReaderPrefs> =
    seriesDirection === 'vertical'
      ? { mode: 'strip' }
      : { direction: seriesDirection === 'ltr' ? 'ltr' : 'rtl' };
  return { ...DEFAULT_READER_PREFS, ...fromSeries, ...s.global, ...s.series[seriesId] };
}
