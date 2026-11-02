/**
 * The NAME of the matched route, never the URL: `/lounge/$guildId/$channelId` becomes `lounge.:guildId.:channelId`.
 * A route pattern is static text of the app, so a name carries no user data; when nothing but the root matched (a page
 * that does not exist) the name is `unmatched`, because the real path would be user-controlled.
 */
export function routeNameOf(routeId: string | undefined): string {
  if (routeId === '/') return 'home';
  if (!routeId || routeId === '__root__') return 'unmatched';
  const parts = routeId
    .split('/')
    .filter(Boolean)
    .map((p) => (p.startsWith('$') ? `:${p.slice(1)}` : p.replace(/^_+|_+$/g, '')))
    .filter(Boolean);
  const name = parts.join('.');
  return name ? name.slice(0, 64) : 'unmatched';
}

/** The route name of a router's current location (the last match). */
export function currentRouteName(router: {
  state: { matches: readonly { routeId: string }[] };
}): string {
  const last = router.state.matches[router.state.matches.length - 1];
  return routeNameOf(last?.routeId);
}
