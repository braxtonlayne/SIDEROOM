import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { Browse, validateBrowseSearch } from './Browse';
// Pages load on demand (XC-PERF-001); the reader alone is the bulk of the section.
const Home = lazyNamed(() => import('./Home'), 'Home');
const UpdatesPage = lazyNamed(() => import('./Pages'), 'UpdatesPage');
const MyLibraryPage = lazyNamed(() => import('./Pages'), 'MyLibraryPage');
const HistoryPage = lazyNamed(() => import('./Pages'), 'HistoryPage');
const SoonPage = lazyNamed(() => import('./Pages'), 'SoonPage');
const Reader = lazyNamed(() => import('./reader/Reader'), 'Reader');
const SeriesPage = lazyNamed(() => import('./SeriesPage'), 'SeriesPage');

const LibraryRoot = lazyNamed(() => import('./LibraryRoot'), 'LibraryRoot');

/**
 * Library routes (spec library-reader section 3):
 *   /library                      Home
 *   /library/browse               Browse (filters live in the search string)
 *   /library/updates              Latest updates
 *   /library/s/:seriesId          Series page
 *   /library/r/:chapterId[/:page] Reader (full screen, outside the section chrome); ?s= is the series id
 *   /library/me, /history, /lists, /requests, /submissions, /upload
 * The section owns its own query cache, the "Are you 18 or older?" gate and the theme.
 */
export function createLibraryRoutes(root: AnyRoute) {
  const layout = createRoute({
    getParentRoute: () => root,
    path: '/library',
    component: LibraryRoot,
  });
  const child = <P extends string>(path: P, component: () => React.JSX.Element | null) =>
    createRoute({ getParentRoute: () => layout, path, component });

  const index = child('/', Home);
  const browse = createRoute({
    getParentRoute: () => layout,
    path: 'browse',
    validateSearch: validateBrowseSearch,
    component: function BrowseRoute() {
      return <Browse search={browse.useSearch()} />;
    },
  });
  const series = createRoute({
    getParentRoute: () => layout,
    path: 's/$seriesId',
    component: function SeriesRoute() {
      const { seriesId } = series.useParams();
      return <SeriesPage key={seriesId} seriesId={seriesId} />;
    },
  });
  const reader = createRoute({
    getParentRoute: () => layout,
    path: 'r/$chapterId/{-$page}',
    component: function ReaderRoute() {
      const { chapterId, page } = reader.useParams();
      return <Reader chapterId={chapterId} page={page} />;
    },
  });
  return layout.addChildren([
    index,
    browse,
    child('updates', UpdatesPage),
    series,
    reader,
    child('me', MyLibraryPage),
    child('history', HistoryPage),
    child('lists', () => <SoonPage page="lists" />),
    child('requests', () => <SoonPage page="requests" />),
    child('submissions', () => <SoonPage page="submissions" />),
    child('upload', () => <SoonPage page="upload" />),
  ]);
}
