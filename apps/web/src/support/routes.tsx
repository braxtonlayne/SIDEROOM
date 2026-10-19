import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const LeaderboardsPage = lazyNamed(() => import('./Leaderboards'), 'LeaderboardsPage');
const SupportPage = lazyNamed(() => import('./SupportPage'), 'SupportPage');

/**
 * /support (DON-UI-001): why donate, the hosting bar, Patron, Server Levels, invite credits and the
 * help answers; /support/leaderboards: the public donor and server boards.
 */
export function createSupportRoutes(root: AnyRoute) {
  return [
    createRoute({ getParentRoute: () => root, path: '/support', component: SupportPage }),
    createRoute({
      getParentRoute: () => root,
      path: '/support/leaderboards',
      component: LeaderboardsPage,
    }),
  ];
}
