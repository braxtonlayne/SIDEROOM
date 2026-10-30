/**
 * UTL-DEV-001: the cryptography of the encrypted scratchpad. Everything here runs in the browser; the server only
 * ever receives the output of `encryptNote`.
 *
 * * The notes key is 256 random bits made once per account. It is kept as a **non-extractable** AES-GCM
 *   `CryptoKey` in IndexedDB (script cannot read its bytes back out), and a copy of the raw bits is wrapped with a
 *   key derived from the member's passphrase (PBKDF2-SHA-256, 600 000 rounds, random salt) into a small JSON
 *   "recovery bundle". The bundle is the only way to read the notes in another browser, so the UI offers it as a
 *   file at setup and says plainly that nobody, the operator included, can recover a lost key or passphrase.
 * * A note is `0x01 || iv(12) || AES-GCM(plaintext)`, with the note id as additional authenticated data, so the
 *   server (or anyone) cannot move a ciphertext to another note id without it failing to open.
 */

export const BUNDLE_VERSION = 1;
export const PBKDF2_ROUNDS = 600_000;
const FORMAT = 1;

export interface RecoveryBundle {
  v: typeof BUNDLE_VERSION;
  kdf: 'PBKDF2-SHA256';
  rounds: number;
  /** Base64 of the 16-byte salt. */
  salt: string;
  /** Base64 of the 12-byte IV. */
  iv: string;
  /** Base64 of the AES-GCM-encrypted 32 key bytes. */
  wrapped: string;
}

export type CryptoProblem = 'bad-passphrase' | 'bad-bundle' | 'bad-ciphertext' | 'unsupported';

export class NotesCryptoError extends Error {
  constructor(public readonly problem: CryptoProblem) {
    super(problem);
    this.name = 'NotesCryptoError';
  }
}

const b64 = (bytes: Uint8Array): string => {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
};
const unb64 = (s: string): Uint8Array => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const buf = (u: Uint8Array): ArrayBuffer => u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength) as ArrayBuffer;

function subtle(): SubtleCrypto {
  const s = globalThis.crypto?.subtle;
  if (!s) throw new NotesCryptoError('unsupported');
  return s;
}

async function passphraseKey(passphrase: string, salt: Uint8Array, rounds: number): Promise<CryptoKey> {
  const s = subtle();
  const base = await s.importKey('raw', buf(new TextEncoder().encode(passphrase.normalize('NFKC'))), 'PBKDF2', false, [
    'deriveKey',
  ]);
  return s.deriveKey(
    { name: 'PBKDF2', hash: 'SHA-256', salt: buf(salt), iterations: rounds },
    base,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  );
}

export interface NewKey {
  /** The non-extractable key to store and use. */
  key: CryptoKey;
  /** The recovery bundle to offer to the member and to store next to the key. */
  bundle: RecoveryBundle;
}

/** Make the account's notes key and wrap a copy of it with the passphrase. */
export async function createNotesKey(passphrase: string, rounds = PBKDF2_ROUNDS): Promise<NewKey> {
  const s = subtle();
  const raw = crypto.getRandomValues(new Uint8Array(32));
  try {
    const salt = crypto.getRandomValues(new Uint8Array(16));
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const wrapKey = await passphraseKey(passphrase, salt, rounds);
    const wrapped = new Uint8Array(await s.encrypt({ name: 'AES-GCM', iv: buf(iv) }, wrapKey, buf(raw)));
    const key = await s.importKey('raw', buf(raw), { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']);
    return {
      key,
      bundle: {
        v: BUNDLE_VERSION,
        kdf: 'PBKDF2-SHA256',
        rounds,
        salt: b64(salt),
        iv: b64(iv),
        wrapped: b64(wrapped),
      },
    };
  } finally {
    raw.fill(0);
  }
}

/** Open a recovery bundle with the passphrase and give back the non-extractable key. */
export async function openRecoveryBundle(bundle: unknown, passphrase: string): Promise<CryptoKey> {
  const b = bundle as Partial<RecoveryBundle> | null;
  if (
    !b ||
    b.v !== BUNDLE_VERSION ||
    b.kdf !== 'PBKDF2-SHA256' ||
    typeof b.rounds !== 'number' ||
    b.rounds < 1 ||
    b.rounds > 10_000_000 ||
    typeof b.salt !== 'string' ||
    typeof b.iv !== 'string' ||
    typeof b.wrapped !== 'string'
  ) {
    throw new NotesCryptoError('bad-bundle');
  }
  const s = subtle();
  let raw: Uint8Array;
  try {
    const wrapKey = await passphraseKey(passphrase, unb64(b.salt), b.rounds);
    raw = new Uint8Array(await s.decrypt({ name: 'AES-GCM', iv: buf(unb64(b.iv)) }, wrapKey, buf(unb64(b.wrapped))));
  } catch {
    // AES-GCM fails its tag check for a wrong passphrase and for a damaged bundle alike.
    throw new NotesCryptoError('bad-passphrase');
  }
  try {
    if (raw.length !== 32) throw new NotesCryptoError('bad-bundle');
    return await s.importKey('raw', buf(raw), { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']);
  } finally {
    raw.fill(0);
  }
}

/** `0x01 || iv || ciphertext`, bound to `noteId`. */
export async function encryptNote(key: CryptoKey, plaintext: string, noteId: string): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(
    await subtle().encrypt(
      { name: 'AES-GCM', iv: buf(iv), additionalData: buf(new TextEncoder().encode(noteId)) },
      key,
      buf(new TextEncoder().encode(plaintext)),
    ),
  );
  const out = new Uint8Array(1 + iv.length + ct.length);
  out[0] = FORMAT;
  out.set(iv, 1);
  out.set(ct, 1 + iv.length);
  return out;
}

export async function decryptNote(key: CryptoKey, bytes: Uint8Array, noteId: string): Promise<string> {
  if (bytes.length < 1 + 12 + 16 || bytes[0] !== FORMAT) throw new NotesCryptoError('bad-ciphertext');
  try {
    const pt = await subtle().decrypt(
      {
        name: 'AES-GCM',
        iv: buf(bytes.subarray(1, 13)),
        additionalData: buf(new TextEncoder().encode(noteId)),
      },
      key,
      buf(bytes.subarray(13)),
    );
    return new TextDecoder().decode(pt);
  } catch {
    throw new NotesCryptoError('bad-ciphertext');
  }
}

export const toBase64 = b64;
export const fromBase64 = unb64;

// ------------------------------------------------------------------ where the key lives

/** Where the key and its bundle are kept in this browser. IndexedDB in the app, memory in tests. */
export interface KeyStorage {
  load(): Promise<{ key: CryptoKey; bundle: RecoveryBundle } | null>;
  save(key: CryptoKey, bundle: RecoveryBundle): Promise<void>;
  clear(): Promise<void>;
}

const DB = 'sr-tools-notes';
const STORE = 'keys';

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error('indexeddb'));
  });
}

function run<T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return open().then(
    (db) =>
      new Promise<T>((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        const req = f(tx.objectStore(STORE));
        tx.oncomplete = () => {
          db.close();
          resolve(req.result);
        };
        tx.onerror = tx.onabort = () => {
          db.close();
          reject(tx.error ?? new Error('indexeddb'));
        };
      }),
  );
}

/** The real thing: a structured-clonable `CryptoKey` in IndexedDB, one record pair per account (two accounts in one browser never share a key). */
export const indexedDbKeys = (userId: string): KeyStorage => ({
  async load() {
    const key = await run('readonly', (s) => s.get(`key:${userId}`) as IDBRequest<CryptoKey | undefined>);
    const bundle = await run(
      'readonly',
      (s) => s.get(`bundle:${userId}`) as IDBRequest<RecoveryBundle | undefined>,
    );
    return key && bundle ? { key, bundle } : null;
  },
  async save(key, bundle) {
    await run('readwrite', (s) => s.put(key, `key:${userId}`));
    await run('readwrite', (s) => s.put(bundle, `bundle:${userId}`));
  },
  async clear() {
    await run('readwrite', (s) => s.delete(`key:${userId}`));
    await run('readwrite', (s) => s.delete(`bundle:${userId}`));
  },
});

export function memoryKeys(): KeyStorage {
  let held: { key: CryptoKey; bundle: RecoveryBundle } | null = null;
  return {
    load: () => Promise.resolve(held),
    save: (key, bundle) => {
      held = { key, bundle };
      return Promise.resolve();
    },
    clear: () => {
      held = null;
      return Promise.resolve();
    },
  };
}
