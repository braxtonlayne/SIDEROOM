// SEC-AUD-039 (D-570, audit theme 10): what a session kept on this device is removed when it ends.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const cleared = vi.hoisted(() => ({ share: 0 }));
vi.mock('./shareFiles', () => ({
  clearSharedFiles: () => {
    cleared.share += 1;
    return Promise.resolve();
  },
}));

import { OFFLINE_CACHE, REGISTRY_KEY } from '../library/reader/offlineCache';
import { NOTES_PURGE_EVENT } from './notesKeys';
import { purgeOnSignOut } from './signOutPurge';

describe('purgeOnSignOut', () => {
  let names: Set<string>;
  beforeEach(() => {
    names = new Set(['sr-app-b1', 'sr-static-b1', OFFLINE_CACHE, 'other']);
    vi.stubGlobal('caches', {
      keys: () => Promise.resolve([...names]),
      delete: (n: string) => Promise.resolve(names.delete(n)),
      open: () => Promise.reject(new Error('unused')),
    });
    localStorage.setItem(REGISTRY_KEY, JSON.stringify(['ch1', 'ch2']));
    cleared.share = 0;
  });
  afterEach(() => vi.unstubAllGlobals());

  it('removes the app files, the shared files and the Library offline chapters, nothing else', async () => {
    await purgeOnSignOut();
    expect([...names].sort()).toEqual(['other', 'sr-static-b1']);
    expect(JSON.parse(localStorage.getItem(REGISTRY_KEY) ?? 'null')).toEqual([]);
    expect(cleared.share).toBe(1);
  });

  it('never rejects when storage is blocked', async () => {
    vi.stubGlobal('caches', {
      keys: () => Promise.reject(new Error('blocked')),
      delete: () => Promise.reject(new Error('blocked')),
    });
    await expect(purgeOnSignOut()).resolves.toBeUndefined();
    expect(cleared.share).toBe(1);
  });
});

// SEC-AUD-136 (wave 61): the Notes key this browser holds goes with the session.
describe('purgeOnSignOut and the Notes key', () => {
  let deleted: string[];
  const fakeIndexedDb = (mode: 'ok' | 'throws' | 'hangs') => ({
    deleteDatabase: (name: string) => {
      if (mode === 'throws') throw new Error('blocked');
      const req: Record<string, unknown> = {};
      if (mode === 'ok')
        setTimeout(() => {
          deleted.push(name);
          (req.onsuccess as () => void)();
        }, 0);
      return req;
    },
  });
  beforeEach(() => {
    deleted = [];
    vi.stubGlobal('caches', {
      keys: () => Promise.resolve([]),
      delete: () => Promise.resolve(true),
      open: () => Promise.reject(new Error('unused')),
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('sign_out_removes_the_notes_key', async () => {
    vi.stubGlobal('indexedDB', fakeIndexedDb('ok'));
    const locked = vi.fn();
    window.addEventListener(NOTES_PURGE_EVENT, locked);
    await purgeOnSignOut();
    window.removeEventListener(NOTES_PURGE_EVENT, locked);
    expect(deleted).toEqual(['sr-tools-notes']);
    // An open Notes store is told to lock.
    expect(locked).toHaveBeenCalledTimes(1);
  });

  it('a_blocked_notes_store_does_not_stop_the_purge', async () => {
    vi.stubGlobal('indexedDB', fakeIndexedDb('throws'));
    await expect(purgeOnSignOut()).resolves.toBeUndefined();
    // A store that never answers: the sign-out waits a moment, not for ever.
    vi.useFakeTimers();
    vi.stubGlobal('indexedDB', fakeIndexedDb('hangs'));
    const done = purgeOnSignOut();
    await vi.advanceTimersByTimeAsync(2500);
    await expect(done).resolves.toBeUndefined();
  });
});
