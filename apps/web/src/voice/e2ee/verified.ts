/**
 * VOX-E2E-007: which participants this user has verified, by identity key. A verification is
 * tied to the key, not to the user: a new key (a new device) shows as unverified again.
 * SEC-AUD-047 (D-578): the mark is kept per person *and* key, so a person verified on their laptop and on their
 * phone keeps both marks (at most {@link MAX_VERIFIED_KEYS} per person, the oldest dropped).
 * Kept per device in localStorage; nothing is sent to the server.
 */
const KEY = 'sr-e2ee-verified';

/** The same cap as the pinned key set (pins.ts). */
export const MAX_VERIFIED_KEYS = 8;

export interface VerifiedStore {
  isVerified(userId: string, keyFingerprint: string): boolean;
  setVerified(userId: string, keyFingerprint: string, verified: boolean): void;
}

/** Person to verified keys (oldest first). Wave 54 stored one key per person as a string. */
function read(storage: Storage | undefined): Record<string, string[]> {
  try {
    const raw = storage?.getItem(KEY);
    const v: unknown = raw ? JSON.parse(raw) : {};
    if (!v || typeof v !== 'object' || Array.isArray(v)) return {};
    const out: Record<string, string[]> = {};
    for (const [user, keys] of Object.entries(v as Record<string, unknown>)) {
      if (typeof keys === 'string') out[user] = [keys];
      else if (Array.isArray(keys))
        out[user] = keys.filter((k): k is string => typeof k === 'string');
    }
    return out;
  } catch {
    return {};
  }
}

export function verifiedStore(
  storage: Storage | undefined = globalThis.localStorage,
): VerifiedStore {
  return {
    isVerified: (userId, fp) => read(storage)[userId]?.includes(fp) === true,
    setVerified: (userId, fp, verified) => {
      const all = read(storage);
      const others = (all[userId] ?? []).filter((k) => k !== fp);
      const keys = verified ? [...others, fp].slice(-MAX_VERIFIED_KEYS) : others;
      if (keys.length) all[userId] = keys;
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
