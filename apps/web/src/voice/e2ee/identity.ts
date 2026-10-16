/**
 * VOX-E2E-005: the account's persistent E2EE identity key (spec section 5.1).
 *
 * An ECDSA P-256 key pair (the curve of the MLS ciphersuite DAVE uses,
 * MLS_128_DHKEMP256_AES128GCM_SHA256_P256) is generated once per account on this device. Its
 * private half is stored only wrapped (AES-GCM) by a *wrapping key* that is itself a
 * non-extractable WebCrypto key kept in IndexedDB: script on the page can use it, but cannot
 * read its bytes, so a dump of the database does not reveal the identity key. The private key
 * is unwrapped as a non-extractable signing key by default; the MLS client (WASM, VOX-E2E-001)
 * will need the PKCS#8 bytes, which `unwrapForExport` gives it.
 *
 * Keys never leave the device. A user signing in on a new device gets a new identity key (and
 * other members see a new privacy code for them), as with DAVE.
 */

export interface StoredIdentity {
  version: 1;
  /** SPKI DER of the public key. */
  publicKey: ArrayBuffer;
  /** PKCS#8 private key, AES-GCM-wrapped. */
  wrappedPrivateKey: ArrayBuffer;
  iv: Uint8Array;
  createdAt: number;
}

/** Where identities and the wrapping key live (IndexedDB in the browser). */
export interface KeyStore {
  getWrappingKey(): Promise<CryptoKey | undefined>;
  putWrappingKey(key: CryptoKey): Promise<void>;
  getIdentity(userId: string): Promise<StoredIdentity | undefined>;
  putIdentity(userId: string, id: StoredIdentity): Promise<void>;
  deleteIdentity(userId: string): Promise<void>;
}

export interface Identity {
  /** SPKI DER; this is what peers see and what the privacy code covers. */
  publicKey: Uint8Array;
  /** Non-extractable ECDSA P-256 signing key. */
  signingKey: CryptoKey;
  /** The public key as 65 raw bytes (uncompressed SEC1), the form MLS credentials carry. */
  rawPublicKey: Uint8Array;
}

async function rawOf(spki: ArrayBuffer | Uint8Array, subtle: SubtleCrypto): Promise<Uint8Array> {
  const k = await subtle.importKey('spki', spki, SIGN, true, ['verify']);
  return new Uint8Array(await subtle.exportKey('raw', k));
}

/** Sign with the identity key: ECDSA P-256 / SHA-256, 64 bytes r||s (what MLS credentials hold). */
export async function signWithIdentity(
  id: Identity,
  message: Uint8Array,
  subtle: SubtleCrypto = globalThis.crypto.subtle,
): Promise<Uint8Array> {
  return new Uint8Array(
    await subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, id.signingKey, message),
  );
}

const SIGN = { name: 'ECDSA', namedCurve: 'P-256' } as const;
const WRAP = { name: 'AES-GCM', length: 256 } as const;

async function wrappingKey(store: KeyStore, subtle: SubtleCrypto): Promise<CryptoKey> {
  const have = await store.getWrappingKey();
  if (have) return have;
  // Non-extractable: its bytes can never be read back out of WebCrypto.
  const key = await subtle.generateKey(WRAP, false, ['wrapKey', 'unwrapKey']);
  await store.putWrappingKey(key);
  return key;
}

async function unwrap(
  stored: StoredIdentity,
  wrap: CryptoKey,
  subtle: SubtleCrypto,
  extractable: boolean,
): Promise<CryptoKey> {
  return subtle.unwrapKey(
    'pkcs8',
    stored.wrappedPrivateKey,
    wrap,
    { name: 'AES-GCM', iv: stored.iv },
    SIGN,
    extractable,
    ['sign'],
  );
}

/** Load this account's identity, creating it on first use. */
export async function loadOrCreateIdentity(
  userId: string,
  store: KeyStore,
  c: Crypto = globalThis.crypto,
  now: () => number = Date.now,
): Promise<Identity> {
  const subtle = c.subtle;
  const wrap = await wrappingKey(store, subtle);
  const stored = await store.getIdentity(userId);
  if (stored) {
    try {
      return {
        publicKey: new Uint8Array(stored.publicKey),
        signingKey: await unwrap(stored, wrap, subtle, false),
        rawPublicKey: await rawOf(stored.publicKey, subtle),
      };
    } catch {
      // The wrapping key was lost or replaced (site data partly cleared): start over.
      await store.deleteIdentity(userId);
    }
  }
  // Extractable only for the moment it takes to wrap it.
  const pair = await subtle.generateKey(SIGN, true, ['sign', 'verify']);
  const iv = c.getRandomValues(new Uint8Array(12));
  const wrapped = await subtle.wrapKey('pkcs8', pair.privateKey, wrap, { name: 'AES-GCM', iv });
  const publicKey = await subtle.exportKey('spki', pair.publicKey);
  await store.putIdentity(userId, {
    version: 1,
    publicKey,
    wrappedPrivateKey: wrapped,
    iv,
    createdAt: now(),
  });
  const fresh = await store.getIdentity(userId);
  if (!fresh) throw new Error('identity key was not stored');
  return {
    publicKey: new Uint8Array(publicKey),
    signingKey: await unwrap(fresh, wrap, subtle, false),
    rawPublicKey: await rawOf(publicKey, subtle),
  };
}

/** The PKCS#8 bytes of the identity key, for the MLS client in WASM (VOX-E2E-001). */
export async function unwrapForExport(
  userId: string,
  store: KeyStore,
  subtle: SubtleCrypto = globalThis.crypto.subtle,
): Promise<Uint8Array | null> {
  const wrap = await store.getWrappingKey();
  const stored = await store.getIdentity(userId);
  if (!wrap || !stored) return null;
  const key = await unwrap(stored, wrap, subtle, true);
  return new Uint8Array(await subtle.exportKey('pkcs8', key));
}

// ---- IndexedDB ----

const DB = 'sr-e2ee';
const KEYS = 'keys';
const IDS = 'identities';
const WRAP_ID = 'wrapping-key';

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error('IndexedDB request failed'));
  });
}

/** The browser store: database `sr-e2ee`, stores `keys` (the wrapping key) and `identities`. */
export function indexedDbKeyStore(idb: IDBFactory = indexedDB): KeyStore {
  let db: Promise<IDBDatabase> | null = null;
  const open = () =>
    (db ??= new Promise<IDBDatabase>((resolve, reject) => {
      const r = idb.open(DB, 1);
      r.onupgradeneeded = () => {
        r.result.createObjectStore(KEYS);
        r.result.createObjectStore(IDS);
      };
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error ?? new Error('IndexedDB open failed'));
    }));
  const store = async (name: string, mode: IDBTransactionMode) =>
    (await open()).transaction(name, mode).objectStore(name);
  return {
    getWrappingKey: async () =>
      (await req((await store(KEYS, 'readonly')).get(WRAP_ID))) as CryptoKey | undefined,
    putWrappingKey: async (key) => {
      await req((await store(KEYS, 'readwrite')).put(key, WRAP_ID));
    },
    getIdentity: async (userId) =>
      (await req((await store(IDS, 'readonly')).get(userId))) as StoredIdentity | undefined,
    putIdentity: async (userId, id) => {
      await req((await store(IDS, 'readwrite')).put(id, userId));
    },
    deleteIdentity: async (userId) => {
      await req((await store(IDS, 'readwrite')).delete(userId));
    },
  };
}

/** An in-memory store (tests, and browsers without IndexedDB, where keys last one session). */
export function memoryKeyStore(): KeyStore {
  let wrap: CryptoKey | undefined;
  const ids = new Map<string, StoredIdentity>();
  return {
    getWrappingKey: () => Promise.resolve(wrap),
    putWrappingKey: (k) => {
      wrap = k;
      return Promise.resolve();
    },
    getIdentity: (u) => Promise.resolve(ids.get(u)),
    putIdentity: (u, id) => {
      ids.set(u, id);
      return Promise.resolve();
    },
    deleteIdentity: (u) => {
      ids.delete(u);
      return Promise.resolve();
    },
  };
}
