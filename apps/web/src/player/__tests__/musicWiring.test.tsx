/**
 * Wave-30 integration: the library panel, the player port and the recent tracker wired to the player store,
 * and the entrance-side gate kept free of the player (D-385).
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getPlayerPort } from '../../music/queue/port';
import { PLAYED_AFTER_MS } from '../../music/queue/recent';
import { __unwireMusicLibrary, createStorePort, wireMusicLibrary } from '../musicWiring';
import { forgetPlayer, isPlayerQueued } from '../presence';
import { __resetPlayerStore, play, RESUME_KEY, usePlayerStore } from '../store';

const T = (n: number) => ({ id: `t${n}`, title: `T${n}`, src: `/t${n}.mp3` });

beforeEach(() => {
  __resetPlayerStore();
  __unwireMusicLibrary();
});
afterEach(() => {
  __unwireMusicLibrary();
  vi.useRealTimers();
});

describe('the store port', () => {
  it('forwards the actions to the store', () => {
    const port = createStorePort();
    port.play([T(1), T(2)], 1);
    expect(usePlayerStore.getState().queue.map((t) => t.id)).toEqual(['t1', 't2']);
    expect(usePlayerStore.getState().index).toBe(1);
    port.enqueue([T(3)], { next: true });
    expect(usePlayerStore.getState().queue.map((t) => t.id)).toEqual(['t1', 't2', 't3']);
    port.setEq([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(usePlayerStore.getState().eq[9]).toBe(10);
    port.setEqEnabled(false);
    expect(usePlayerStore.getState().eqEnabled).toBe(false);
    port.setCrossfade(30);
    expect(usePlayerStore.getState().crossfadeSec).toBe(12);
  });

  it('keeps one snapshot while nothing it shows changes (useSyncExternalStore)', () => {
    const port = createStorePort();
    const first = port.getState();
    usePlayerStore.setState({ position: 12 }); // not part of the port's view
    expect(port.getState()).toBe(first);
    play([T(1)]);
    const second = port.getState();
    expect(second).not.toBe(first);
    expect(second.queueIds).toEqual(['t1']);
    expect(second.index).toBe(0);
  });
});

describe('wireMusicLibrary', () => {
  it('registers the panel and the port once, and records a play after five seconds', async () => {
    vi.useFakeTimers();
    const record = vi.fn(() => Promise.resolve());
    wireMusicLibrary({ record });
    wireMusicLibrary({ record }); // a host remount: no second tracker
    expect(usePlayerStore.getState().libraryPanel).not.toBeNull();
    getPlayerPort().play([T(7)]);
    expect(usePlayerStore.getState().queue).toHaveLength(1);
    vi.advanceTimersByTime(PLAYED_AFTER_MS + 10);
    expect(record).toHaveBeenCalledTimes(1);
    expect(record).toHaveBeenCalledWith('t7');
  });

  it('skipping through the queue does not fill the history', () => {
    vi.useFakeTimers();
    const record = vi.fn(() => Promise.resolve());
    wireMusicLibrary({ record });
    play([T(1), T(2), T(3)]);
    vi.advanceTimersByTime(1000);
    play(1);
    vi.advanceTimersByTime(1000);
    play(2);
    vi.advanceTimersByTime(PLAYED_AFTER_MS + 10);
    expect(record.mock.calls).toEqual([['t3']]);
  });

  it('unwiring puts the inert port back and removes the panel', () => {
    wireMusicLibrary({ record: () => Promise.resolve() });
    __unwireMusicLibrary();
    expect(usePlayerStore.getState().libraryPanel).toBeNull();
    getPlayerPort().play([T(1)]);
    expect(usePlayerStore.getState().queue).toHaveLength(0);
  });
});

describe('presence (the entrance side of the player)', () => {
  it('mirrors the queue and forgets it on sign-out', () => {
    play([T(1)]);
    expect(isPlayerQueued()).toBe(true);
    sessionStorage.setItem(RESUME_KEY, 'x');
    forgetPlayer();
    expect(usePlayerStore.getState().queue).toHaveLength(0);
    expect(isPlayerQueued()).toBe(false);
    expect(sessionStorage.getItem(RESUME_KEY)).toBeNull();
  });

  it('PlayerGate, presence and the sign-out path never import the player store, engine or music code', () => {
    const src = (p: string) => readFileSync(join(__dirname, '..', p), 'utf8');
    const staticImports = (code: string) =>
      [...code.matchAll(/^import[^'"]*['"]([^'"]+)['"]/gm)].map((m) => m[1]);
    for (const file of ['PlayerGate.tsx', 'presence.ts', 'scope.ts']) {
      const imports = staticImports(src(file));
      expect(
        imports.filter((i) => /\/(store|engine|musicWiring|PlayerHost)$|music\//.test(i!)),
        file,
      ).toEqual([]);
    }
    const signOut = staticImports(src('../settings/user/api.ts'));
    expect(signOut.some((i) => i!.includes('player/store'))).toBe(false);
  });
});
