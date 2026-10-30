/**
 * Addresses of the images a public profile shows: avatars, banners and server icons are content-hash addressed
 * public kinds under /cdn and need no signature. A copy of the rule in `store/cdn` (an app module this surface may
 * not import, D-385): the longest edge is rounded up to a power of two from 16 to 4096.
 */
export function cdnAddress(
  kind: 'avatars' | 'icons',
  owner: string,
  hash: string | null | undefined,
  size: number,
): string | null {
  if (!hash) return null;
  const n = Math.min(4096, Math.max(16, Math.ceil(size)));
  return `/cdn/${kind}/${encodeURIComponent(owner)}/${encodeURIComponent(hash)}.png?size=${2 ** Math.ceil(Math.log2(n))}`;
}
