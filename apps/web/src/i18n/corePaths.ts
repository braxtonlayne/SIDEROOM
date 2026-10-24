/**
 * Paths that render from the core catalog fragments alone (entrance, a11y, pwa), so they paint before the
 * rest of the catalog arrives. Every other path waits behind `CatalogGate` (see `RootLayout` in
 * '../router.tsx'). A path NOT listed here is safe by default: it just waits. Adding a path here means
 * every key it uses must be in `messages/en/{entrance,a11y,pwa}.ts`; `corePaths.test.ts` enforces that
 * for the entrance route components.
 */
const EXACT = new Set([
  '/',
  '/login',
  '/login/2fa',
  '/login/link',
  '/register',
  '/forgot',
  '/no-invitation',
  '/house-notes',
  '/email-change/cancel',
  '/oauth/discord/callback',
]);
const PREFIXES = ['/join/', '/reset/', '/verify-email/', '/invite/', '/template/'];

export function isCorePath(pathname: string): boolean {
  const p = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname;
  return EXACT.has(p) || PREFIXES.some((x) => p.startsWith(x));
}
