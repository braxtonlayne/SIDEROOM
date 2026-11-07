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
