import { MAX_PATH } from './contract';

const SHAPE = new RegExp(`^/[a-z0-9_@:./-]{1,${MAX_PATH}}$`);

/** A segment that is an id or a secret: all digits, or 16+ of hex/base64url, or anything with upper case (an invite code). */
function isIdLike(seg: string): boolean {
  if (/^[0-9]+$/.test(seg)) return true;
  if (seg.length >= 16 && /^[A-Za-z0-9_-]+$/.test(seg)) return true;
  // Mixed or upper case never appears in a route word (they are lower case), so it is a code or a name: not ours to keep.
  return /[A-Z]/.test(seg);
}

/**
 * The path template of a same-origin `/api/` URL, or `null`: the origin, query and fragment are dropped, every id-like
 * segment becomes `:id` (`@me` is kept), and a result the server's check would drop is dropped here first.
 */
export function templateOf(url: string, origin?: string): string | null {
  if (typeof url !== 'string' || url.length > 2048) return null;
  const here = origin ?? (typeof location === 'object' ? location.origin : '');
  let u: URL;
  try {
    u = new URL(url, here || undefined);
  } catch {
    return null;
  }
  if (!here || u.origin !== here) return null;
  if (u.pathname !== '/api' && !u.pathname.startsWith('/api/')) return null;
  let segments: string[];
  try {
    segments = u.pathname.split('/').map((s) => decodeURIComponent(s));
  } catch {
    return null;
  }
  const path = segments.map((s) => (s === '' || s === '@me' || !isIdLike(s) ? s : ':id')).join('/');
  if (!SHAPE.test(path) || /\d{6}/.test(path)) return null;
  return path;
}
