// Live TV routes (TV-UI, TV-PLY). Mounted inside the Cinema `/cinema` layout by cinema/routes.tsx:
//   /cinema/tv                       the guide
//   /cinema/tv/watch/$channel        the live player (full screen, outside the section chrome: CinemaRoot.isPlayer)
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../../lib/lazyRoute';

const TvGuidePage = lazyNamed(() => import('./Guide'), 'TvGuidePage');
const LiveWatchRoute = lazyNamed(() => import('./LiveWatch'), 'LiveWatchRoute');

/** Children for the Cinema section's `/cinema` layout route (relative paths). */
export function createTvRoutes<P extends AnyRoute>(parent: P) {
  const guide = createRoute({ getParentRoute: () => parent, path: 'tv', component: TvGuidePage });
  const watch = createRoute({
    getParentRoute: () => parent,
    path: 'tv/watch/$channel',
    component: LiveWatchRoute,
  });
  return [guide, watch] as const;
}
