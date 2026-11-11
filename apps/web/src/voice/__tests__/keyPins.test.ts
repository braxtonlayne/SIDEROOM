// SEC-AUD-038 (D-564), SEC-AUD-047 (D-578): identity keys are pinned per person as a set (cap 8, oldest dropped).
// The first key is pinned silently, a key in the set is silent, a key new to the set is reported once and joins
// it; the verified mark is per person and key.
import { describe, expect, it, vi } from 'vitest';
import {
  MAX_KEYS_PER_PERSON,
  acceptKey,
  asKeySet,
  checkKeys,
  memoryPinStore,
  type PinStore,
} from '../e2ee/pins';
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

const seen = (key: string) => [{ userId: '2', identityKey: key }];
const keysOf = async (pins: PinStore, user = '2') =>
  asKeySet(await pins.get(ME, user)).map((k) => k.key);

describe('key pins (D-564, D-578)', () => {
  it('pins the first key seen and reports nothing for it', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    expect(await checkKeys(pins, ME, seen('aa'), verified)).toEqual([]);
    expect(await keysOf(pins)).toEqual(['aa']);
    expect(await checkKeys(pins, ME, seen('aa'), verified)).toEqual([]);
  });

  it('a key new to the set is reported once, joins the set and has no verified mark', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, seen('aa'), verified);
    verified.setVerified('2', 'aa', true);
    expect(await checkKeys(pins, ME, seen('bb'), verified)).toEqual(['2']);
    expect(await keysOf(pins)).toEqual(['aa', 'bb']);
    // The mark is per person and key: the laptop stays verified, the new device is not.
    expect(verified.isVerified('2', 'aa')).toBe(true);
    expect(verified.isVerified('2', 'bb')).toBe(false);
    expect(await checkKeys(pins, ME, seen('bb'), verified)).toEqual([]);
  });

  it('two devices of one person no longer flip-flop the notice', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, seen('aa'), verified);
    await checkKeys(pins, ME, seen('bb'), verified);
    for (const key of ['aa', 'bb', 'aa', 'bb'])
      expect(await checkKeys(pins, ME, seen(key), verified)).toEqual([]);
  });

  it('keeps at most eight keys per person, dropping the oldest', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    let t = 0;
    const now = () => ++t;
    for (let i = 0; i <= MAX_KEYS_PER_PERSON; i++)
      await checkKeys(pins, ME, seen(`k${i}`), verified, now);
    const keys = await keysOf(pins);
    expect(keys).toHaveLength(MAX_KEYS_PER_PERSON);
    expect(keys).not.toContain('k0');
    // The dropped key is new again.
    expect(await checkKeys(pins, ME, seen('k0'), verified, now)).toEqual(['2']);
  });

  it('marking verified pins the key again as the newest, so the cap keeps it', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, seen('k0'), verified);
    for (let i = 1; i < MAX_KEYS_PER_PERSON; i++)
      await checkKeys(pins, ME, seen(`k${i}`), verified);
    await acceptKey(pins, ME, '2', 'k0');
    await checkKeys(pins, ME, seen('k8'), verified);
    const keys = await keysOf(pins);
    expect(keys).toContain('k0');
    expect(keys).not.toContain('k1');
  });

  it('a wave 54 single pin reads as a set of one', async () => {
    const pins = memoryPinStore();
    const raw = new Map<string, unknown>([[`${ME}:2`, { key: 'aa', pinnedAt: 5 }]]);
    const legacy: PinStore = {
      get: (v, u) =>
        raw.has(`${v}:${u}`)
          ? Promise.resolve(raw.get(`${v}:${u}`) as { key: string; pinnedAt: number })
          : pins.get(v, u),
      put: (v, u, p) => {
        raw.delete(`${v}:${u}`);
        return pins.put(v, u, p);
      },
    };
    const verified = verifiedStore(memoryStorage());
    expect(await checkKeys(legacy, ME, seen('aa'), verified)).toEqual([]);
    expect(await checkKeys(legacy, ME, seen('bb'), verified)).toEqual(['2']);
    expect(asKeySet(await legacy.get(ME, '2')).map((k) => k.key)).toEqual(['aa', 'bb']);
  });

  it('pins are per viewer and never cover the viewer themself', async () => {
    const pins = memoryPinStore();
    const verified = verifiedStore(memoryStorage());
    await checkKeys(pins, ME, seen('aa'), verified);
    // Another account on the same browser starts with its own pins.
    expect(await checkKeys(pins, '9', seen('bb'), verified)).toEqual([]);
    expect(await checkKeys(pins, ME, [{ userId: ME, identityKey: 'zz' }], verified)).toEqual([]);
    expect(await pins.get(ME, ME)).toBeUndefined();
  });

  it('blocked storage neither throws nor trusts anything silently', async () => {
    const broken: PinStore = {
      get: () => Promise.reject(new Error('blocked')),
      put: () => Promise.reject(new Error('blocked')),
    };
    const verified = verifiedStore(memoryStorage());
    await expect(checkKeys(broken, ME, seen('aa'), verified)).resolves.toEqual([]);
  });

  it('the call binding keeps a reported person on the notice for the rest of the call', async () => {
    resetE2ee();
    const pins = memoryPinStore();
    await pins.put(ME, '2', { keys: [{ key: 'aa', pinnedAt: 1 }] });
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
    // The next roster finds the key in the set (silent), and the notice stays for this call.
    listener!({ ...state, epoch: 2 });
    await vi.waitFor(async () => expect(await keysOf(pins)).toEqual(['aa', 'bb']));
    await new Promise((r) => setTimeout(r, 0));
    expect(useE2ee.getState().keyChanged).toEqual(['2']);
    unbind();
    resetE2ee();
  });
});

describe('verified marks per person and key (D-578)', () => {
  it('keeps one mark per key and reads the wave 54 single mark', () => {
    const storage = memoryStorage();
    storage.setItem('sr-e2ee-verified', JSON.stringify({ '2': 'aa' }));
    const v = verifiedStore(storage);
    expect(v.isVerified('2', 'aa')).toBe(true);
    v.setVerified('2', 'bb', true);
    expect(v.isVerified('2', 'aa')).toBe(true);
    expect(v.isVerified('2', 'bb')).toBe(true);
    v.setVerified('2', 'aa', false);
    expect(v.isVerified('2', 'aa')).toBe(false);
    expect(v.isVerified('2', 'bb')).toBe(true);
  });
});
