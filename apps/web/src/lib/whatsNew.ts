// RDY-ONB-003: the version this build embeds (the newest heading of the root CHANGELOG.md, set in vite.config.ts)
// and the one-time badge on the help button. A browser remembers the version it last showed the news for.
import { useSyncExternalStore } from 'react';

const SEEN_KEY = 'sr.whatsNew.seen';

/** The version of this build: "Phase 22". `dev` when the build had no changelog. */
export const APP_VERSION: string = typeof __SR_VERSION__ === 'string' ? __SR_VERSION__ : 'dev';

const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

function read(): string | null {
  try {
    return localStorage.getItem(SEEN_KEY);
  } catch {
    return null;
  }
}

function write(v: string) {
  try {
    localStorage.setItem(SEEN_KEY, v);
  } catch {
    // Storage blocked: the badge then shows until the page is closed, which is harmless.
  }
}

/**
 * Whether the badge shows. A browser that has never seen any version is a new install: it records the current
 * one without showing anything, so a new member is not greeted by news about changes they never saw.
 */
export function hasUnseenNews(version = APP_VERSION): boolean {
  const seen = read();
  if (seen === null) {
    write(version);
    return false;
  }
  return seen !== version;
}

/** Called when the person opens `/help/whats-new`: the badge goes away until the version changes again. */
export function markNewsSeen(version = APP_VERSION) {
  write(version);
  emit();
}

/**
 * Another tab of the same browser opened what's new (or recorded the version): this tab's badge follows, without a
 * reload (RDY-PWA-001: after an accepted update every open tab is on the new version, and only one of them is read).
 */
function onStorage(e: StorageEvent) {
  if (e.key === SEEN_KEY || e.key === null) emit();
}

function subscribe(cb: () => void): () => void {
  if (listeners.size === 0 && typeof window !== 'undefined') {
    window.addEventListener('storage', onStorage);
  }
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
    if (listeners.size === 0 && typeof window !== 'undefined') {
      window.removeEventListener('storage', onStorage);
    }
  };
}

export function useNewsBadge(): boolean {
  return useSyncExternalStore(
    subscribe,
    // `hasUnseenNews` records the version on a first visit, then answers the same until the version changes.
    () => hasUnseenNews(),
    () => false,
  );
}
