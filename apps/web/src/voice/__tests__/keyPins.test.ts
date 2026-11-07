// SEC-AUD-038 (D-564): identity keys are pinned on first sight; a changed key drops the verified mark and stays
// flagged until the viewer re-verifies.
import { describe, expect, it, vi } from 'vitest';
import { acceptKey, checkKeys, memoryPinStore, type PinStore } from '../e2ee/pins';
import { bindToStore } from '../e2ee/runtime';
import { resetE2ee, useE2ee } from '../e2ee/store';
import { verifiedStore } from '../e2ee/verified';
import type { E2eeCall, E2eeCallState } from '../e2ee/call';
import type { E2eeApi } from '@sideroom/wasm';

const ME = '1';

function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    get length() {
      return m.size;
    },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => void m.delete(k),
    setItem: (k, v) => void m.set(k, v),
  };
}

describe('key pins (D-564)', () => {
  it('pins the first key seen and reports nothing for it', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    expect(await checkKeys(pins, ME, [{ userId: '2', identityKey: 'aa' }], verified)).toEqual([]);
    expect((await pins.get(ME, '2'))?.key).toBe('aa');
    expect(await checkKeys(pins, ME, [{ userId: '2', identityKey: 'aa' }], verified)).toEqual([]);
  });

  it('a different key is reported and drops the verified mark', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, [{ userId: '2', identityKey: 'aa' }], verified);
    verified.setVerified('2', 'aa', true);
    const changed = await checkKeys(pins, ME, [{ userId: '2', identityKey: 'bb' }], verified);
    expect(changed).toEqual(['2']);
    expect(verified.isVerified('2', 'aa')).toBe(false);
    expect(verified.isVerified('2', 'bb')).toBe(false);
    // The pin is not moved by seeing the new key: the notice stays until the person is re-verified.
    expect((await pins.get(ME, '2'))?.key).toBe('aa');
    expect(await checkKeys(pins, ME, [{ userId: '2', identityKey: 'bb' }], verified)).toEqual([
      '2',
    ]);
  });

  it('re-verifying trusts the new key and clears the notice', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, [{ userId: '2', identityKey: 'aa' }], verified);
    await checkKeys(pins, ME, [{ userId: '2', identityKey: 'bb' }], verified);
    await acceptKey(pins, ME, '2', 'bb');
    expect(await checkKeys(pins, ME, [{ userId: '2', identityKey: 'bb' }], verified)).toEqual([]);
  });

  it('pins are per viewer and never cover the viewer themself', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, [{ userId: '2', identityKey: 'aa' }], verified);
    // Another account on the same browser starts with its own pins.
    expect(await checkKeys(pins, '9', [{ userId: '2', identityKey: 'bb' }], verified)).toEqual([]);
    expect(await checkKeys(pins, ME, [{ userId: ME, identityKey: 'zz' }], verified)).toEqual([]);
    expect(await pins.get(ME, ME)).toBeUndefined();
  });

  it('blocked storage neither throws nor trusts anything silently', async () => {
    const broken: PinStore = {
      get: () => Promise.reject(new Error('blocked')),
      put: () => Promise.reject(new Error('blocked')),
    };
    const verified = verifiedStore(memoryStorage());
    await expect(
      checkKeys(broken, ME, [{ userId: '2', identityKey: 'aa' }], verified),
    ).resolves.toEqual([]);
  });

  it('the call binding publishes who changed key to the shield store', async () => {
    resetE2ee();
    const pins = memoryPinStore();
    await pins.put(ME, '2', { key: 'aa', pinnedAt: 1 });
    let listener: ((s: E2eeCallState) => void) | null = null;
    const state: E2eeCallState = {
      status: 'secure',
      epoch: 1,
      callCode: 'c',
      members: [
        { userId: ME, identityKey: '11' },
        { userId: '2', identityKey: 'bb' },
      ],
      error: null,
    };
    const call = {
      state,
      subscribe: (fn: (s: E2eeCallState) => void) => {
        listener = fn;
        return () => (listener = null);
      },
    } as unknown as E2eeCall;
    const api = { pairwiseCode: () => '00000' } as unknown as E2eeApi;
    const unbind = bindToStore(
      call,
      api,
      ME,
      new Uint8Array([1]),
      pins,
      verifiedStore(memoryStorage()),
    );
    expect(useE2ee.getState().viewer).toBe(ME);
    listener!(state);
    await vi.waitFor(() => expect(useE2ee.getState().keyChanged).toEqual(['2']));
    unbind();
    resetE2ee();
  });
});
