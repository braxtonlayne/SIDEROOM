import { useCallback, useEffect, useRef, useState } from 'react';
import { apiFetch } from '../api/client';
import { onDispatch } from '../store/events';
import { usePrefsStore, type NavMode } from '../settings/user/prefs';

/**
 * `GET /users/@me/shelves/summary` (BYOS-SCP-001): counts per kind of what the viewer can reach through
 * shelves. `catalog` (counts of the instance catalog visible to the viewer) is optional (null or absent when the count query fails) so Auto can tell
 * "nothing anywhere" from "nothing on shelves"; when it is absent Auto keeps the tab (fail visible).
 */
export interface ShelvesSummary {
  cinema: number;
  library: number;
  music: number;
  catalog?: { cinema: number; library: number } | null;
  /** The browser converter's policy (P29-CNV-1); absent from an older server. */
  convert?: ConvertPolicy;
}

/** `byos.client_convert_max_mib` and `byos.require_client_remux`, as the summary reports them. */
export interface ConvertPolicy {
  max_mib: number;
  require_client_remux: boolean;
}
export const getShelvesSummary = () =>
  apiFetch<ShelvesSummary>('/users/@me/shelves/summary', { retries: 0 });

/** Gateway events after which the summary is fetched again (BYOS-NAV-001). */
export const SUMMARY_EVENTS: readonly string[] = ['READY', 'STORAGE_MOUNT_UPDATE'];

/** Pure rule: show/hide are absolute; Auto hides only when both the shelves and the instance catalog are known empty. */
export function tabVisible(
  mode: NavMode,
  kind: 'cinema' | 'library',
  summary: ShelvesSummary | null,
): boolean {
  if (mode === 'show') return true;
  if (mode === 'hide') return false;
  if (!summary || !summary.catalog) return true;
  return summary[kind] > 0 || summary.catalog[kind] > 0;
}

/**
 * The shelves summary while any tab is on Auto: fetched once, then again after READY (a reconnect may have
 * missed changes) and on `STORAGE_MOUNT_UPDATE`. A failed fetch keeps the last known summary (fail visible).
 * `null` while unknown or when nothing needs it.
 */
export function useShelvesSummary(enabled: boolean): ShelvesSummary | null {
  const [summary, setSummary] = useState<ShelvesSummary | null>(null);
  const seq = useRef(0);
  const load = useCallback(() => {
    const mine = ++seq.current;
    getShelvesSummary()
      .then((s) => {
        if (mine === seq.current) setSummary(s);
      })
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    if (!enabled) return;
    load();
    const off = onDispatch((event) => {
      if (SUMMARY_EVENTS.includes(event)) load();
    });
    return () => {
      off();
      seq.current += 1;
    };
  }, [enabled, load]);
  return summary;
}

/** Which of the Cinema and Library top tabs to show. The summary is fetched only while a tab is on Auto. */
export function useNavVisibility(): { cinema: boolean; library: boolean } {
  const cinemaMode = usePrefsStore((s) => s.prefs.navCinema);
  const libraryMode = usePrefsStore((s) => s.prefs.navLibrary);
  const summary = useShelvesSummary(cinemaMode === 'auto' || libraryMode === 'auto');
  return {
    cinema: tabVisible(cinemaMode, 'cinema', summary),
    library: tabVisible(libraryMode, 'library', summary),
  };
}
