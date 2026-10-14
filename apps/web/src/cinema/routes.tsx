import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { validateBrowseSearch } from './browseSearch';

// Pages load on demand (XC-PERF-001).
const CinemaRoot = lazyNamed(() => import('./CinemaRoot'), 'CinemaRoot');
const Browse = lazyNamed(() => import('./Browse'), 'Browse');
const Discover = lazyNamed(() => import('./Discover'), 'Discover');
const TitlePage = lazyNamed(() => import('./TitlePage'), 'TitlePage');

/**
 * Cinema routes (spec cinema-media section 2):
 *   /cinema                 Discover
 *   /cinema/movies|series|anime   Browse (filters live in the search string)
 *   /cinema/t/:id           Title detail
 *   /cinema/search?q=       Search results
 * Not here (other agents): /cinema/watch/:id and /cinema/my-list (player agent). To add one, create the route
 * in its own file and append it to the `addChildren` list below; keep each child definition self-contained.
 * The player route renders without the section chrome: see CinemaRoot (`isPlayer`).
 */
export function createCinemaRoutes(root: AnyRoute) {
  const layout = createRoute({
    getParentRoute: () => root,
    path: '/cinema',
    component: CinemaRoot,
  });

  // ---- browse-agent routes (p6-cinema-browse) ----
  const index = createRoute({ getParentRoute: () => layout, path: '/', component: Discover });
  const browse = (page: 'movies' | 'series' | 'anime') => {
    const route = createRoute({
      getParentRoute: () => layout,
      path: page,
      validateSearch: validateBrowseSearch,
      component: function BrowseRoute() {
        return <Browse page={page} search={route.useSearch()} />;
      },
    });
    return route;
  };
  const search = createRoute({
    getParentRoute: () => layout,
    path: 'search',
    validateSearch: validateBrowseSearch,
    component: function SearchRoute() {
      return <Browse page="search" search={search.useSearch()} />;
    },
  });
  const title = createRoute({
    getParentRoute: () => layout,
    path: 't/$id',
    component: function TitleRoute() {
      const { id } = title.useParams();
      return <TitlePage key={id} id={id} />;
    },
  });
  // ---- end browse-agent routes ----

  return layout.addChildren([
    index,
    browse('movies'),
    browse('series'),
    browse('anime'),
    search,
    title,
  ]);
}
