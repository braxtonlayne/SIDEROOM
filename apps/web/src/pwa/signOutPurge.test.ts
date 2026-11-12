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
  let cleared: number;
  /** An IndexedDB of just enough shape: one database with the `keys` store. */
  const fakeIndexedDb = (mode: 'ok' | 'throws' | 'hangs') => ({
    open: () => {
      if (mode === 'throws') throw new Error('blocked');
      const req: Record<string, unknown> = {};
      if (mode === 'ok')
        setTimeout(() => {
          const tx: Record<string, unknown> = {
            objectStore: () => ({
              delete: (k: string) => deleted.push(k),
              clear: () => (cleared += 1),
            }),
          };
          req.result = {
            objectStoreNames: { contains: () => true },
            transaction: () => {
              setTimeout(() => (tx.oncomplete as () => void)(), 0);
              return tx;
            },
            close: () => undefined,
          };
          (req.onsuccess as () => void)();
        }, 0);
      return req;
    },
  });
  beforeEach(() => {
    deleted = [];
    cleared = 0;
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
    await purgeOnSignOut('u1');
    expect(deleted.sort()).toEqual(['bundle:u1', 'key:u1']);
    expect(cleared).toBe(0);
  });

  it('removes every account key when the account is not known (the session ended elsewhere)', async () => {
    vi.stubGlobal('indexedDB', fakeIndexedDb('ok'));
    await purgeOnSignOut();
    expect(cleared).toBe(1);
    expect(deleted).toEqual([]);
  });

  it('a_blocked_notes_store_does_not_stop_the_purge', async () => {
    vi.stubGlobal('indexedDB', fakeIndexedDb('throws'));
    await expect(purgeOnSignOut('u1')).resolves.toBeUndefined();
    expect(cleared).toBe(0);
    // A store that never answers: the sign-out waits a moment, not for ever.
    vi.useFakeTimers();
    vi.stubGlobal('indexedDB', fakeIndexedDb('hangs'));
    const done = purgeOnSignOut('u1');
    await vi.advanceTimersByTimeAsync(2500);
    await expect(done).resolves.toBeUndefined();
  });
});
