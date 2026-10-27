import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { createShelfRoutes } from '../storage/shelves/routes';
import { lazyNamed } from '../lib/lazyRoute';
import { Browse, validateBrowseSearch } from './Browse';
// Pages load on demand (XC-PERF-001); the reader alone is the bulk of the section.
const Home = lazyNamed(() => import('./Home'), 'Home');
const UpdatesPage = lazyNamed(() => import('./Pages'), 'UpdatesPage');
const MyLibraryPage = lazyNamed(() => import('./Pages'), 'MyLibraryPage');
const HistoryPage = lazyNamed(() => import('./Pages'), 'HistoryPage');
const ListsPage = lazyNamed(() => import('./Lists'), 'ListsPage');
const ListPage = lazyNamed(() => import('./Lists'), 'ListPage');
const RankingsPage = lazyNamed(() => import('./Rankings'), 'RankingsPage');
const SoonPage = lazyNamed(() => import('./Pages'), 'SoonPage');
const Reader = lazyNamed(() => import('./reader/Reader'), 'Reader');
const SeriesPage = lazyNamed(() => import('./SeriesPage'), 'SeriesPage');
const RequestsBoard = lazyNamed(() => import('../review/RequestsBoard'), 'RequestsBoard');
const SubmissionsPage = lazyNamed(() => import('./Submissions'), 'SubmissionsPage');
const SubmissionPage = lazyNamed(() => import('./Submissions'), 'SubmissionPage');

const LibraryRoot = lazyNamed(() => import('./LibraryRoot'), 'LibraryRoot');

/**
 * Library routes (spec library-reader section 3):
 *   /library                      Home
 *   /library/browse               Browse (filters live in the search string)
 *   /library/updates              Latest updates
 *   /library/s/:seriesId          Series page
 *   /library/r/:chapterId[/:page] Reader (full screen, outside the section chrome); ?s= is the series id
 *   /library/rankings             Rankings (week, month, all time)
 *   /library/lists[/:listId]      Reading lists and one list
 *   /library/me, /history, /requests, /upload
 *   /library/submissions[?series=ID]  My submissions;  /library/submissions/:submissionId  one submission
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
  const list = createRoute({
    getParentRoute: () => layout,
    path: 'lists/$listId',
    component: function ListRoute() {
      const { listId } = list.useParams();
      return <ListPage key={listId} listId={listId} />;
    },
  });
  const submissions = createRoute({
    getParentRoute: () => layout,
    path: 'submissions',
    validateSearch: (s: Record<string, unknown>): { series?: string } => {
      // The router parses ?series=123 into a number.
      const v =
        typeof s.series === 'number' || typeof s.series === 'string' ? String(s.series) : '';
      return /^[0-9]+$/.test(v) ? { series: v } : {};
    },
    component: function SubmissionsRoute() {
      return <SubmissionsPage series={submissions.useSearch().series} />;
    },
  });
  const submission = createRoute({
    getParentRoute: () => layout,
    path: 'submissions/$submissionId',
    component: function SubmissionRoute() {
      const { submissionId } = submission.useParams();
      return <SubmissionPage key={submissionId} id={submissionId} />;
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
    child('lists', ListsPage),
    list,
    child('rankings', RankingsPage),
    child('requests', () => <RequestsBoard />),
    submissions,
    submission,
    child('upload', () => <SoonPage page="upload" />),
    ...createShelfRoutes(layout, 'library'),
  ]);
}
