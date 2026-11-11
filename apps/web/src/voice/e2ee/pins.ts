/**
 * SEC-AUD-038 (D-564), SEC-AUD-047 (D-578): trust on first use for E2EE identity keys, on this device only.
 *
 * Each account has one identity key per device (`identity.ts`, as with DAVE), so a person with a phone and a
 * laptop legitimately has two privacy codes. This viewer keeps a set of the keys it has seen for each person in
 * IndexedDB (database `sr-e2ee-pins`, one record per viewer and person, at most {@link MAX_KEYS_PER_PERSON} keys,
 * the oldest dropped). The first key seen is pinned silently. A key already in the set is silent. A key not in
 * it is reported once (the shield says the person has a new privacy code: a new device or a new sign-in), joins
 * the set, and carries no verified mark until the viewer compares the code and marks the person verified, which
 * pins that key again as the newest. The server never pins and there is no setting: a server-side pin would make
 * the server the trust anchor that end-to-end encryption excludes.
 *
 * A substituted key is still a key this viewer has not seen, so it is still flagged; nothing is accepted
 * silently except the very first key of a person.
 */
import type { VerifiedStore } from './verified';

/** Keys kept per person (D-578). */
export const MAX_KEYS_PER_PERSON = 8;

export interface PinnedKey {
  /** Hex of an identity key this viewer has seen (and so trusts as one of the person's devices). */
  key: string;
  pinnedAt: number;
}

export interface KeyPins {
  /** Oldest first, newest last; never more than {@link MAX_KEYS_PER_PERSON}. */
  keys: PinnedKey[];
}

/** Wave 54 stored one key per person; it reads as a set of one. */
interface LegacyPin {
  key: string;
  pinnedAt: number;
}

/** Where pins live: IndexedDB in the browser, a map in tests and where IndexedDB is missing. */
export interface PinStore {
  get(viewer: string, userId: string): Promise<KeyPins | LegacyPin | undefined>;
  put(viewer: string, userId: string, pins: KeyPins): Promise<void>;
}

export interface SeenMember {
  userId: string;
  /** Hex of the identity public key the call carries for this person. */
  identityKey: string;
}

/** A stored record as a set (the wave 54 single pin becomes a set of one; anything malformed is empty). */
export function asKeySet(raw: KeyPins | LegacyPin | undefined): PinnedKey[] {
  if (!raw || typeof raw !== 'object') return [];
  if ('keys' in raw && Array.isArray(raw.keys))
    return raw.keys.filter(
      (k): k is PinnedKey => !!k && typeof k.key === 'string' && typeof k.pinnedAt === 'number',
    );
  if ('key' in raw && typeof raw.key === 'string')
    return [{ key: raw.key, pinnedAt: typeof raw.pinnedAt === 'number' ? raw.pinnedAt : 0 }];
  return [];
}

/** The set with `key` added (or moved) as the newest, cut to the newest {@link MAX_KEYS_PER_PERSON}. */
export function withKey(keys: readonly PinnedKey[], key: string, at: number): PinnedKey[] {
  return [...keys.filter((k) => k.key !== key), { key, pinnedAt: at }].slice(-MAX_KEYS_PER_PERSON);
}

/**
 * Compare the call's keys with the pinned sets: pin the first key seen for a person silently, stay silent for a
 * key already in the person's set, and report (once) every person whose key is new to the set. A reported key
 * joins the set and has no verified mark. Returns the user ids that carry a new key.
 */
export async function checkKeys(
  store: PinStore,
  viewer: string,
  members: readonly SeenMember[],
  verified: VerifiedStore,
  now: () => number = Date.now,
): Promise<string[]> {
  const changed: string[] = [];
  for (const m of members) {
    if (m.userId === viewer || !m.identityKey) continue;
    let keys: PinnedKey[];
    try {
      keys = asKeySet(await store.get(viewer, m.userId));
    } catch {
      // Storage blocked: nothing to compare with, and nothing is trusted silently either.
      continue;
    }
    if (keys.some((k) => k.key === m.identityKey)) continue;
    if (keys.length > 0) {
      // A mark left on this key from before it was dropped from the set does not carry over.
      verified.setVerified(m.userId, m.identityKey, false);
      changed.push(m.userId);
    }
    try {
      await store.put(viewer, m.userId, { keys: withKey(keys, m.identityKey, now()) });
    } catch {
      /* blocked storage: the next call compares again */
    }
  }
  return changed;
}

/** The viewer compared the code and marked the person verified: that key is pinned again as the newest. */
export async function acceptKey(
  store: PinStore,
  viewer: string,
  userId: string,
  identityKey: string,
  now: () => number = Date.now,
): Promise<void> {
  const keys = asKeySet(await store.get(viewer, userId));
  await store.put(viewer, userId, { keys: withKey(keys, identityKey, now()) });
}

const slot = (viewer: string, userId: string) => `${viewer}:${userId}`;

export function memoryPinStore(): PinStore {
  const pins = new Map<string, KeyPins | LegacyPin>();
  return {
    get: (v, u) => Promise.resolve(pins.get(slot(v, u))),
    put: (v, u, p) => {
      pins.set(slot(v, u), p);
      return Promise.resolve();
    },
  };
}

const DB = 'sr-e2ee-pins';
const STORE = 'pins';

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error('IndexedDB request failed'));
  });
}

/** The browser store: database `sr-e2ee-pins`, store `pins`, keyed `<viewer>:<user>`. */
export function indexedDbPinStore(idb: IDBFactory = indexedDB): PinStore {
  let db: Promise<IDBDatabase> | null = null;
  const open = () =>
    (db ??= new Promise<IDBDatabase>((resolve, reject) => {
      const r = idb.open(DB, 1);
      r.onupgradeneeded = () => {
        if (!r.result.objectStoreNames.contains(STORE)) r.result.createObjectStore(STORE);
      };
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error ?? new Error('IndexedDB open failed'));
    }));
  const store = async (mode: IDBTransactionMode) =>
    (await open()).transaction(STORE, mode).objectStore(STORE);
  return {
    get: async (v, u) =>
      (await req((await store('readonly')).get(slot(v, u)))) as KeyPins | LegacyPin | undefined,
    put: async (v, u, p) => {
      await req((await store('readwrite')).put(p, slot(v, u)));
    },
  };
}

let shared: PinStore | null = null;
/** This device's pins (IndexedDB, or memory where IndexedDB is unavailable). */
export function devicePins(): PinStore {
  if (!shared) {
    try {
      shared = typeof indexedDB === 'undefined' ? memoryPinStore() : indexedDbPinStore();
    } catch {
      shared = memoryPinStore();
    }
  }
  return shared;
}
