// ARC-GST-002: sealing the lobby's data with the ephemeral game key. The relay is unkeyed (it holds no key and no
// MLS group), so every payload that crosses it is AES-256-GCM under the key that only travels in the link's fragment.
// Layout: nonce(12) || ciphertext+tag. The lobby token is the associated data, so a sealed message cannot be moved
// to another lobby.
import { fromBase64Url } from './link';

const NONCE = 12;
const enc = new TextEncoder();

/** Copies into a fresh ArrayBuffer-backed view (WebCrypto wants BufferSource, not a SharedArrayBuffer view). */
const buf = (b: Uint8Array): Uint8Array => new Uint8Array(b);

export async function importGameKey(key: string): Promise<CryptoKey> {
  return crypto.subtle.importKey('raw', buf(fromBase64Url(key)), 'AES-GCM', false, [
    'encrypt',
    'decrypt',
  ]);
}

export async function seal(
  key: CryptoKey,
  lobbyToken: string,
  plain: Uint8Array,
): Promise<Uint8Array> {
  const nonce = crypto.getRandomValues(new Uint8Array(NONCE));
  const ct = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv: nonce, additionalData: buf(enc.encode(lobbyToken)) },
      key,
      buf(plain),
    ),
  );
  const out = new Uint8Array(NONCE + ct.length);
  out.set(nonce, 0);
  out.set(ct, NONCE);
  return out;
}

/** `null` when the bytes were not sealed under this key for this lobby. */
export async function open(
  key: CryptoKey,
  lobbyToken: string,
  sealed: Uint8Array,
): Promise<Uint8Array | null> {
  if (sealed.length < NONCE + 16) return null;
  try {
    const plain = await crypto.subtle.decrypt(
      {
        name: 'AES-GCM',
        iv: buf(sealed.slice(0, NONCE)),
        additionalData: buf(enc.encode(lobbyToken)),
      },
      key,
      buf(sealed.slice(NONCE)),
    );
    return new Uint8Array(plain);
  } catch {
    return null;
  }
}
