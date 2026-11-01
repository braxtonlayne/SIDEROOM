// ARC-GST-002: the Share link. `/play/<lobby_token>#key=<ephemeral_game_key>`; the key is made in this browser and
// sits in the URL fragment, which a browser never sends to a server (RFC 3986 section 3.5), so the relay and the
// server see the token and ciphertext only.
import { GAME_KEY, LOBBY_TOKEN } from './contract';

const B64URL = /^[A-Za-z0-9_-]*$/;

export function toBase64Url(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function fromBase64Url(s: string): Uint8Array {
  if (!B64URL.test(s)) throw new Error('not base64url');
  const padded = s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4);
  const bin = atob(padded);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** 32 random bytes from the platform's CSPRNG, as a url-safe string (43 characters). */
export function generateGameKey(): string {
  return toBase64Url(crypto.getRandomValues(new Uint8Array(32)));
}

export function buildGuestLink(origin: string, lobbyToken: string, gameKey: string): string {
  if (!LOBBY_TOKEN.test(lobbyToken)) throw new Error('bad lobby token');
  if (!GAME_KEY.test(gameKey)) throw new Error('bad game key');
  return `${origin.replace(/\/+$/, '')}/play/${lobbyToken}#key=${gameKey}`;
}

export interface ParsedGuestLink {
  token: string;
  /** `null` when the fragment has no valid key (the page cannot read the game; it says so). */
  key: string | null;
}

/** Reads the page's own location. `null` when the path is not `/play/<token>`. */
export function parseGuestLocation(pathname: string, hash: string): ParsedGuestLink | null {
  const m = /^\/play\/([^/]+)\/?$/.exec(pathname);
  if (!m) return null;
  let token: string;
  try {
    token = decodeURIComponent(m[1]!);
  } catch {
    return null;
  }
  if (!LOBBY_TOKEN.test(token)) return null;
  const k = /(?:^#?|&)key=([^&]*)/.exec(hash)?.[1] ?? '';
  return { token, key: GAME_KEY.test(k) ? k : null };
}
