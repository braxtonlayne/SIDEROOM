// Route of the watch party theatre: `/cinema/party/$id?code=<invite code>`. The Cinema layout mounts it as a child
// (see cinema/routes.tsx) and skips its chrome for this path, like the player.
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const PartyRoute = lazyNamed(() => import('./PartyRoute'), 'PartyRoute');

export const parsePartySearch = (s: Record<string, unknown>): { code?: string } =>
  typeof s.code === 'string' && s.code ? { code: s.code } : {};

/** Children for the Cinema section's `/cinema` layout route (relative path). */
export const createWatchPartyRoutes = <P extends AnyRoute>(parent: P) => [
  createRoute({
    getParentRoute: () => parent,
    path: 'party/$id',
    validateSearch: parsePartySearch,
    component: PartyRoute,
  }),
];

export const PARTY_PATH_PREFIX = '/cinema/party/';
