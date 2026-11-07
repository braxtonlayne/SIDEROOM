/**
 * SEC-AUD-038 (D-564): trust on first use for E2EE identity keys, on this device only.
 *
 * The first identity key this viewer sees for a person is pinned in IndexedDB (database `sr-e2ee-pins`, one
 * record per viewer and person). When a call later carries a different key for that person, the verified mark
 * is dropped and the shield shows "privacy code changed" until the viewer compares the new code and marks the
 * person verified again, which pins the new key. The server never pins and there is no setting: a server-side
 * pin would make the server the trust anchor that end-to-end encryption excludes.
 *
 * A new key is often harmless (a new device or a cleared browser gets a new identity key, as with DAVE), but it
 * is also what a substituted key looks like, so the notice stays until the person is checked again.
 */
import type { VerifiedStore } from './verified';

export interface KeyPin {
  /** Hex of the identity key this viewer trusts for the person (the first seen, or the last re-verified). */
  key: string;
  pinnedAt: number;
}

/** Where pins live: IndexedDB in the browser, a map in tests and where IndexedDB is missing. */
export interface PinStore {
  get(viewer: string, userId: string): Promise<KeyPin | undefined>;
  put(viewer: string, userId: string, pin: KeyPin): Promise<void>;
}

export interface SeenMember {
  userId: string;
  /** Hex of the identity public key the call carries for this person. */
  identityKey: string;
}

/**
 * Compare the call's keys with the pins: pin the first key seen for a person, and report every person whose
 * key differs from the pinned one (their verified mark is dropped). Returns the user ids whose key changed.
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
    let pin: KeyPin | undefined;
    try {
      pin = await store.get(viewer, m.userId);
    } catch {
      // Storage blocked: nothing to compare with, and nothing is trusted silently either.
      continue;
    }
    if (!pin) {
      try {
        await store.put(viewer, m.userId, { key: m.identityKey, pinnedAt: now() });
      } catch {
        /* blocked storage: the next call pins it */
      }
      continue;
    }
    if (pin.key !== m.identityKey) {
      // The mark was given to the old key; the new key has none until the person is checked again.
      verified.setVerified(m.userId, pin.key, false);
      verified.setVerified(m.userId, m.identityKey, false);
      changed.push(m.userId);
    }
  }
  return changed;
}

/** The viewer compared the new code and marked the person verified: trust the key the call carries now. */
export async function acceptKey(
  store: PinStore,
  viewer: string,
  userId: string,
  identityKey: string,
  now: () => number = Date.now,
): Promise<void> {
  await store.put(viewer, userId, { key: identityKey, pinnedAt: now() });
}

const slot = (viewer: string, userId: string) => `${viewer}:${userId}`;

export function memoryPinStore(): PinStore {
  const pins = new Map<string, KeyPin>();
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
      (await req((await store('readonly')).get(slot(v, u)))) as KeyPin | undefined,
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
