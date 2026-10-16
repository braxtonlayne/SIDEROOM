/**
 * VOX-E2E-007: which participants this user has verified, by identity key. A verification is
 * tied to the key, not to the user: a new key (a new device) shows as unverified again.
 * Kept per device in localStorage; nothing is sent to the server.
 */
const KEY = 'sr-e2ee-verified';

export interface VerifiedStore {
  isVerified(userId: string, keyFingerprint: string): boolean;
  setVerified(userId: string, keyFingerprint: string, verified: boolean): void;
}

function read(storage: Storage | undefined): Record<string, string> {
  try {
    const raw = storage?.getItem(KEY);
    const v: unknown = raw ? JSON.parse(raw) : {};
    return v && typeof v === 'object' ? (v as Record<string, string>) : {};
  } catch {
    return {};
  }
}

export function verifiedStore(
  storage: Storage | undefined = globalThis.localStorage,
): VerifiedStore {
  return {
    isVerified: (userId, fp) => read(storage)[userId] === fp,
    setVerified: (userId, fp, verified) => {
      const all = read(storage);
      if (verified) all[userId] = fp;
      else delete all[userId];
      try {
        storage?.setItem(KEY, JSON.stringify(all));
      } catch {
        // Storage full or blocked: the mark lasts until reload.
      }
    },
  };
}

/** Hex SHA-256 of an identity public key, the stable id a verification is stored under. */
export async function keyFingerprint(
  publicKey: Uint8Array,
  subtle: SubtleCrypto = globalThis.crypto.subtle,
): Promise<string> {
  const d = new Uint8Array(await subtle.digest('SHA-256', publicKey));
  return Array.from(d, (b) => b.toString(16).padStart(2, '0')).join('');
}
