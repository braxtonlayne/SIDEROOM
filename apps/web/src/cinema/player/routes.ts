// Routes of the player and the personal pages (CIN-PLY, CIN-MY-001).
//
//   /cinema/watch/$id?ep=<episodeId>&t=<seconds>   full-screen player
//   /cinema/my-list                                 saved titles, continue watching, lists, history
//
// The Cinema section (p6-cinema-browse) mounts them inside its `/cinema` layout route with one line:
//
//   layout.addChildren([ ...browseRoutes, ...createPlayerRoutes(layout) ])
//
// and its layout must skip its chrome (header, footer) for paths that start with `/cinema/watch/`, the way the
// Library does for its reader (the player is full screen). The layout also supplies the query client, theme and
// `<AgeGateProvider>` that the pages use.
//
// Until that layout exists, `router.tsx` mounts `createPlayerRootRoutes(root)`: the same pages on the root route,
// wrapped in their own frame (theme, query cache, age gate). Swap that line for the one above when browse lands.
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../../lib/lazyRoute';
import { parseWatchSearch, type WatchSearch } from './model';

const PlayerFrame = lazyNamed(() => import('./PlayerFrame'), 'PlayerFrame');
const WatchRoute = lazyNamed(() => import('./WatchRoute'), 'WatchRoute');
const MyListPage = lazyNamed(() => import('../my/MyListPage'), 'MyListPage');

function build<P extends AnyRoute, W extends string, L extends string>(
  parent: P,
  watchPath: W,
  listPath: L,
) {
  const watch = createRoute({
    getParentRoute: () => parent,
    path: watchPath,
    validateSearch: (s: Record<string, unknown>): WatchSearch => parseWatchSearch(s),
    component: WatchRoute,
  });
  const myList = createRoute({
    getParentRoute: () => parent,
    path: listPath,
    component: MyListPage,
  });
  return [watch, myList] as const;
}

/** Children for the Cinema section's `/cinema` layout route (relative paths). */
export const createPlayerRoutes = <P extends AnyRoute>(parent: P) =>
  build(parent, 'watch/$id', 'my-list');

/** Standalone mount on the root route (absolute paths, own frame). */
export function createPlayerRootRoutes<P extends AnyRoute>(root: P) {
  const frame = createRoute({
    getParentRoute: () => root,
    id: 'cinema-player-frame',
    component: PlayerFrame,
  });
  return frame.addChildren(build(frame, '/cinema/watch/$id', '/cinema/my-list'));
}
