/**
 * The hand-off from a public surface to the app (D-385, entrance-isolation.md section 4).
 *
 * Every sign-in success path ends in a full document navigation (`location.assign`), never a router
 * transition: the app is a separate gated build and the public bundle must not know any section's path.
 * `/app` is the neutral door; the app's own router sends the member to the section they last used.
 */

/** The neutral door of the app. The only app URL any public surface knows. */
export const APP_DOOR = '/app';

const MAX_NEXT = 2048;

/**
 * `next` is a destination the server wrote into `/login?next=`. Only a same-origin path is honoured: one
 * leading `/`, no `//`, no backslash, no control character, under 2 KB. It is the same rule the server
 * applies when it writes `next` (crates/sr-server web.rs); both are tested with the same cases.
 */
export function safeNext(next: string | undefined | null): string | null {
  if (typeof next !== 'string' || next.length === 0 || next.length > MAX_NEXT) return null;
  if (!next.startsWith('/') || next.startsWith('//')) return null;
  // eslint-disable-next-line no-control-regex
  if (/[\\\u0000-\u001f\u007f]/.test(next)) return null;
  return next;
}

/** Leave for the app (or a validated `next`) with a document load. */
export function enterApp(next?: string | null): void {
  window.location.assign(safeNext(next) ?? APP_DOOR);
}

/** Leave for the sign-in page, remembering where the person was (the API client's 401 exit uses this). */
export function exitToLogin(current: string = window.location.pathname + window.location.search) {
  const safe = safeNext(current);
  window.location.assign(safe ? `/login?next=${encodeURIComponent(safe)}` : '/login');
}
