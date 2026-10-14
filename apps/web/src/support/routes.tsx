import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const LeaderboardsPage = lazyNamed(() => import('./Leaderboards'), 'LeaderboardsPage');

/** /support/leaderboards: the public donor and server boards (the `/support` help page stays as it was). */
export function createSupportRoutes(root: AnyRoute) {
  return [
    createRoute({
      getParentRoute: () => root,
      path: '/support/leaderboards',
      component: LeaderboardsPage,
    }),
  ];
}
