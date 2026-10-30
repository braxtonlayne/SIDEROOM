import { isCorePath } from '../i18n/corePaths';

/**
 * MUS-PLY-001: the player belongs to the signed-in app only. A path is public when it is an entrance page
 * (the same list the router uses to paint from the core catalog: landing, login, register, reset, invite and
 * template pages), the handbook (`/help`, which holds the legal and what's-new pages), a public profile
 * (`/u/<name>`, `/~<handle>`) or an OAuth consent screen. Everything else is the signed-in app: Lounge,
 * Cinema, Library, Settings, Admin, Developers, Support. (The activity embed is served from its own host and
 * never runs this router.)
 */
export function isPublicPath(pathname: string): boolean {
  const p = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname;
  return (
    isCorePath(p) ||
    p === '/help' ||
    p.startsWith('/help/') ||
    p.startsWith('/u/') ||
    p.startsWith('/~') ||
    p.startsWith('/oauth2/')
  );
}
