import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  createRoute,
  Outlet,
  useRouter,
  useRouterState,
  type AnyRoute,
} from '@tanstack/react-router';
import { useState, type MouseEvent } from 'react';
import { AgeGateProvider } from '../lib/ageGate';
import { useAppliedTheme } from '../settings/user/prefs';
import { Browse, validateBrowseSearch } from './Browse';
import { Home } from './Home';
import { HistoryPage, MyLibraryPage, SoonPage, UpdatesPage } from './Pages';
import { Reader } from './reader/Reader';
import { SeriesPage } from './SeriesPage';
import { LibraryShell, useRequireSession } from './Shell';
import './library.css';

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
function Frame() {
  const reading = useRouterState({ select: (s) => s.location.pathname.startsWith('/library/r/') });
  const unauthorized = useRequireSession();
  if (unauthorized) return null;
  return reading ? <Outlet /> : <LibraryShell />;
}

function LibraryRoot() {
  const theme = useAppliedTheme();
  const router = useRouter();
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: import.meta.env.MODE === 'test' ? false : 1,
            refetchOnWindowFocus: false,
          },
        },
      }),
  );

  // The shared UI kit renders plain anchors (PosterCard, ArrowLink): route in-app links without a reload.
  const onClick = (e: MouseEvent<HTMLElement>) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey)
      return;
    const a = (e.target as HTMLElement).closest?.('a');
    const href = a?.getAttribute('href');
    if (
      !a ||
      !href ||
      a.target ||
      a.hasAttribute('download') ||
      !href.startsWith('/') ||
      href.startsWith('//')
    )
      return;
    e.preventDefault();
    router.history.push(href);
  };

  return (
    <QueryClientProvider client={client}>
      <ThemeProvider
        section="library"
        appearance={theme.appearance}
        style={theme.style}
        className="lib-root"
        onClick={onClick}
      >
        <ToastProvider>
          <AgeGateProvider>
            <Frame />
          </AgeGateProvider>
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

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
