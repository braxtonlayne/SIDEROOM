// Platform announcements (ADM-PLT-009, client side). There is no gateway push for them yet, so the list is
// read when the app starts and then polled. Dismissals are a per-browser convenience (localStorage may be
// unavailable, then a dismissed notice simply comes back on the next load).
import { useCallback, useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';

export type AnnouncementSeverity = 'info' | 'warning' | 'critical';
export interface PlatformAnnouncement {
  id: string;
  title: string;
  body: string;
  severity: AnnouncementSeverity;
  starts_at_ms: number;
  ends_at_ms?: number | null;
}

export const ANNOUNCEMENT_POLL_MS = 5 * 60_000;
const DISMISSED_KEY = 'sr.announcements.dismissed';

export const fetchActiveAnnouncements = () =>
  apiFetch<PlatformAnnouncement[]>('/announcements/active', { retries: 0 });

function readDismissed(): string[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(DISMISSED_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

function writeDismissed(ids: string[]) {
  try {
    // Keep the list short: old ids of ended announcements never come back.
    localStorage.setItem(DISMISSED_KEY, JSON.stringify(ids.slice(-50)));
  } catch {
    /* storage blocked */
  }
}

/** Critical notices cannot be dismissed. */
export const canDismiss = (a: PlatformAnnouncement) => a.severity !== 'critical';

export function useActiveAnnouncements(pollMs: number = ANNOUNCEMENT_POLL_MS, enabled = true) {
  const [all, setAll] = useState<PlatformAnnouncement[]>([]);
  const [dismissed, setDismissed] = useState<string[]>(readDismissed);

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    const load = () =>
      fetchActiveAnnouncements()
        .then((list) => live && setAll(Array.isArray(list) ? list : []))
        // Signed out, offline or a server without the route: keep what is shown.
        .catch(() => undefined);
    void load();
    const timer = setInterval(() => void load(), pollMs);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [pollMs, enabled]);

  const dismiss = useCallback((id: string) => {
    setDismissed((cur) => {
      if (cur.includes(id)) return cur;
      const next = [...cur, id];
      writeDismissed(next);
      return next;
    });
  }, []);

  const visible = all.filter((a) => !canDismiss(a) || !dismissed.includes(a.id));
  return { announcements: visible, dismiss };
}
