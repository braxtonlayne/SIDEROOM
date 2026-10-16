// FND-STOR-003: URLs for profile and guild images served by `/cdn` (see docs/05 section 5.2).
//
//   cdnUrl('avatars', userId, hash, { size: 64 })   -> /cdn/avatars/<user>/<hash>.png?size=64
//   cdnUrl('icons', guildId, hash, { size: 128 })   -> /cdn/icons/<guild>/<hash>.png?size=128
//
// `avatars` also serves user banners, `icons` also serves guild banners and splashes, `channel-icons` serves
// group DM icons (`/cdn/channel-icons/<channel>/<hash>`) and `guild-events` serves scheduled event covers
// (`/cdn/guild-events/<event>/<hash>`).
// These kinds are addressed by an unguessable content hash and need no signature.
// Animated images carry an `a_` hash prefix; they stay GIF unless a static format is asked for.

export type CdnKind = 'avatars' | 'icons' | 'channel-icons' | 'guild-events';
export type CdnFormat = 'png' | 'jpg' | 'webp' | 'gif';

export interface CdnUrlOptions {
  /** Longest edge in px: a power of two from 16 to 4096. Other values are rounded up to one. */
  size?: number;
  /** Output format; default png (webp for static images when `preferWebp`). */
  format?: CdnFormat;
  /** Ask for WebP when the image is static and no format is given. */
  preferWebp?: boolean;
  /** Public origin of the CDN, no trailing slash (default: same origin). */
  base?: string;
}

/** Nearest valid `size`: a power of two in 16..4096, rounding up. */
export function cdnSize(size: number): number {
  const n = Math.min(4096, Math.max(16, Math.ceil(size)));
  return 2 ** Math.ceil(Math.log2(n));
}

export function isAnimatedHash(hash: string): boolean {
  return hash.startsWith('a_');
}

/** URL of a stored image, or null when there is no hash. */
export function cdnUrl(
  kind: CdnKind,
  ownerId: string,
  hash: string | null | undefined,
  opts: CdnUrlOptions = {},
): string | null {
  if (!hash) return null;
  const format = opts.format ?? (opts.preferWebp && !isAnimatedHash(hash) ? 'webp' : 'png');
  const q = opts.size === undefined ? '' : `?size=${cdnSize(opts.size)}`;
  return `${opts.base ?? ''}/cdn/${kind}/${encodeURIComponent(ownerId)}/${encodeURIComponent(hash)}.${format}${q}`;
}
