// LNG-RAIL-014: the rail's "NEW" badge. A server is new for 24 hours after the member joined it and until they
// first open it. "Opened" is kept per browser (localStorage, never sent anywhere); the join time is the member's
// own `joined_at`, which the client already has.
import { useCallback, useSyncExternalStore } from 'react';
import { useStore } from '../store';

const KEY = 'sr.lounge.openedServers';
const DAY_MS = 24 * 60 * 60 * 1000;
const listeners = new Set<() => void>();
let cache: ReadonlySet<string> | null = null;

function read(): ReadonlySet<string> {
  if (cache) return cache;
  let ids: string[] = [];
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    if (Array.isArray(raw)) ids = raw.filter((x): x is string => typeof x === 'string');
  } catch {
    /* unreadable or blocked storage: nothing is known to be opened */
  }
  cache = new Set(ids);
  return cache;
}

/** Remember that the member has opened this server. */
export function markServerOpened(guildId: string): void {
  if (read().has(guildId)) return;
  // Newest 200 only: an old server is long past its 24 hours anyway.
  const next = new Set([...read(), guildId]);
  cache = new Set([...next].slice(-200));
  try {
    localStorage.setItem(KEY, JSON.stringify([...cache]));
  } catch {
    /* blocked storage: the badge then ends at the 24 hours */
  }
  listeners.forEach((l) => l());
}

/** Test hook: forget what was read and opened. */
export function resetOpenedServers(): void {
  cache = null;
  listeners.forEach((l) => l());
}

/** True when the member joined less than 24 hours ago and has not opened the server. */
export function isNewServer(
  joinedAt: string | undefined,
  opened: boolean,
  now = Date.now(),
): boolean {
  if (opened || !joinedAt) return false;
  const at = Date.parse(joinedAt);
  return Number.isFinite(at) && at <= now + 60_000 && now - at < DAY_MS;
}

/** Whether the rail shows "NEW" on this server. */
export function useIsNewServer(guildId: string): boolean {
  const joinedAt = useStore((s) =>
    s.currentUserId ? s.members[`${guildId}:${s.currentUserId}`]?.joined_at : undefined,
  );
  const subscribe = useCallback((cb: () => void) => {
    listeners.add(cb);
    return () => listeners.delete(cb);
  }, []);
  const opened = useSyncExternalStore(subscribe, () => read().has(guildId));
  return isNewServer(joinedAt, opened);
}
