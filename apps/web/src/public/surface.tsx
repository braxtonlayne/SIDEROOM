import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  parseSearchWith,
  RouterProvider,
  stringifySearchWith,
  useRouterState,
  type AnyRoute,
  type RouterHistory,
} from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { LiveRegions } from '../a11y/announcer';
import { EntranceShell } from '../components/EntranceShell';
import { initLocale, useT } from '../i18n';
import { parseSearchValue, stringifySearchValue } from '../lib/searchCodec';
import { useNoindex } from '../lib/useNoindex';
import { registerServiceWorker } from '../pwa/register';
import '../styles/fonts.css';
import '../styles/tokens.css';
import '../styles/a11y.css';

/**
 * Shared scaffolding of the public surfaces (D-385): each surface's `main.tsx` calls `mountSurface` with its own
 * routes. Nothing here may name an app path; a path that is not this surface's gets the entrance's "nothing
 * here" page, never a redirect to an app route.
 */

function PublicRoot() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  useNoindex(pathname);
  return <Outlet />;
}

function NothingHere() {
  const t = useT();
  return (
    <EntranceShell nav="back-to-entrance">
      <main className="ent-prose">
        <h1>{t('entrance.notFound.title')}</h1>
        <p>{t('entrance.notFound.body')}</p>
      </main>
    </EntranceShell>
  );
}

export type RouteFactory = <P extends string>(
  path: P,
  component: () => React.JSX.Element | null,
) => AnyRoute;
export type SurfaceRoutes = (root: AnyRoute, route: RouteFactory) => AnyRoute[];

/** A router of one surface's routes. Exported for the tests of each surface. */
export function createSurfaceRouter(build: SurfaceRoutes, history?: RouterHistory) {
  const root = createRootRoute({ component: PublicRoot });
  const route: RouteFactory = (path, component) =>
    createRoute({ getParentRoute: () => root, path, component });
  return createRouter({
    routeTree: root.addChildren(build(root, route)),
    history,
    defaultNotFoundComponent: NothingHere,
    parseSearch: parseSearchWith(parseSearchValue),
    stringifySearch: stringifySearchWith(stringifySearchValue, parseSearchValue),
  });
}

/** Restore the saved language, paint from the core catalog, register the worker. */
export function mountSurface(router: ReturnType<typeof createSurfaceRouter>): void {
  void initLocale().finally(() => {
    createRoot(document.getElementById('root')!).render(
      <StrictMode>
        <RouterProvider router={router} />
        <LiveRegions />
      </StrictMode>,
    );
    registerServiceWorker();
  });
}
