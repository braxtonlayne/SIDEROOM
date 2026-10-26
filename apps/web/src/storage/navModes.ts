import { useEffect, useState } from 'react';
import { apiFetch } from '../api/client';
import { usePrefsStore, type NavMode } from '../settings/user/prefs';

/**
 * `GET /users/@me/shelves/summary` (BYOS-SCP-001): counts per kind of what the viewer can reach through
 * shelves. `catalog` (counts of the instance catalog visible to the viewer) is ASSUMED so Auto can tell
 * "nothing anywhere" from "nothing on shelves"; when it is absent Auto keeps the tab (fail visible).
 */
export interface ShelvesSummary {
  cinema: number;
  library: number;
  music: number;
  catalog?: { cinema: number; library: number };
}
export const getShelvesSummary = () =>
  apiFetch<ShelvesSummary>('/users/@me/shelves/summary', { retries: 0 });

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

/** Which of the Cinema and Library top tabs to show. The summary is fetched only while a tab is on Auto. */
export function useNavVisibility(): { cinema: boolean; library: boolean } {
  const cinemaMode = usePrefsStore((s) => s.prefs.navCinema);
  const libraryMode = usePrefsStore((s) => s.prefs.navLibrary);
  const [summary, setSummary] = useState<ShelvesSummary | null>(null);
  const needSummary = cinemaMode === 'auto' || libraryMode === 'auto';
  useEffect(() => {
    if (!needSummary) return;
    let live = true;
    getShelvesSummary()
      .then((s) => live && setSummary(s))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [needSummary]);
  return {
    cinema: tabVisible(cinemaMode, 'cinema', summary),
    library: tabVisible(libraryMode, 'library', summary),
  };
}
