import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  parseSearchWith,
  stringifySearchWith,
  useRouterState,
  type RouterHistory,
} from '@tanstack/react-router';
import { lazy, Suspense } from 'react';
import { useT } from './i18n';
import { CatalogGate } from './i18n/CatalogGate';
import { RouteFallback } from './lib/lazyRoute';
import { useNoindex } from './lib/useNoindex';
import { createCinemaRoutes } from './cinema/routes';
import { createLibraryRoutes } from './library/routes';
import { createLoungeRoutes } from './routes/lounge';
import { createUserSettingsRoutes } from './settings/user/routes';
import { createServerSettingsRoutes } from './routes/settingsServer';
import { createAdminRoutes } from './admin/routes';
import { createDeveloperRoutes } from './developers/routes';
import { createSupportRoutes } from './support/routes';
import { createProfilePageRoutes } from './profilepage/routes';
import { createPwaRoutes } from './pwa/routes';
import { createStorageRoutes } from './storage/routes';
import { createEmuRoutes } from './emu/routes';
import { isPublicPath } from './player/scope';
import { parseSearchValue, stringifySearchValue } from './lib/searchCodec';
import { Help } from './help/HelpPage';
import { HelpOperators } from './help/Operators';
import { HelpSkeleton } from './help/HelpSkeleton';
import { AppServerInvite } from './app/AppServerInvite';
import { AppServerTemplate } from './app/AppServerTemplate';
import { AppDoor } from './app/AppDoor';

// /help/whats-new carries the whole changelog text, so it is its own chunk (RDY-ONB-003).
const WhatsNewPage = lazy(() => import('./help/WhatsNew').then((m) => ({ default: m.WhatsNew })));
function WhatsNew() {
  return (
    <Suspense fallback={<HelpSkeleton />}>
      <WhatsNewPage />
    </Suspense>
  );
}

// The re-auth dialog (ADM-STF-009) lives at the root so staff-card buttons in the Lounge can open the
// second-factor window too; it is its own chunk and renders nothing until asked.
const ReauthHost = lazy(() =>
  import('./admin/ReauthHost').then((m) => ({ default: m.ReauthHost })),
);
// The floating player (MUS-PLY-001) belongs to the signed-in app: its gate is its own chunk and is not even
// requested on a public page (D-385: a visitor without a login loads only what that page needs).
const PlayerGate = lazy(() =>
  import('./player/PlayerGate').then((m) => ({ default: m.PlayerGate })),
);
// The utility drawer, its floating gadgets and the pin rail (UTL-SHL-001) follow the same rule as the player: a
// chunk of the signed-in app, never requested on a public page (D-385).
const DrawerGate = lazy(() =>
  import('./tools/drawer/DrawerGate').then((m) => ({ default: m.DrawerGate })),
);
// The handbook waits for the full catalog (it is not a first-paint path); it shows the entrance skeleton meanwhile.
const isHelpPath = (p: string) => p === '/help' || p.startsWith('/help/');

function RootLayout() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  useNoindex(pathname);
  return (
    <>
      {/* Every app route waits for the full catalog (the public surfaces paint from the core fragments alone). */}
      <CatalogGate fallback={isHelpPath(pathname) ? <HelpSkeleton /> : <RouteFallback />}>
        <Outlet />
      </CatalogGate>
      <CatalogGate>
        <Suspense fallback={null}>
          <ReauthHost />
        </Suspense>
        {/* The floating player lives here, above every route, so playback survives navigation (MUS-PLY-001). */}
        {!isPublicPath(pathname) && (
          <Suspense fallback={null}>
            <PlayerGate />
          </Suspense>
        )}
        {/* The drawer host lives here too, so gadgets survive navigation (UTL-SHL-001). */}
        {!isPublicPath(pathname) && (
          <Suspense fallback={null}>
            <DrawerGate />
          </Suspense>
        )}
      </CatalogGate>
    </>
  );
}
const root = createRootRoute({ component: RootLayout });
const r = <P extends string>(path: P, component: () => React.JSX.Element | null) =>
  createRoute({ getParentRoute: () => root, path, component });

/** The neutral door (D-385): `/app` sends the member to where they last were, or to `?goto=` a path of the app. */
const appDoorRoute = createRoute({
  getParentRoute: () => root,
  path: '/app',
  component: AppDoor,
  validateSearch: (s: Record<string, unknown>): { goto?: string } =>
    typeof s.goto === 'string' ? { goto: s.goto } : {},
});

const routeTree = root.addChildren([
  appDoorRoute,
  r('/', AppDoor),
  r('/help', Help),
  r('/help/whats-new', WhatsNew),
  r('/help/operators', HelpOperators),
  r('/invite/$code', AppServerInvite),
  r('/template/$code', AppServerTemplate),
  createLoungeRoutes(root),
  ...createUserSettingsRoutes(root),
  ...createServerSettingsRoutes(root),
  createCinemaRoutes(root),
  createLibraryRoutes(root),
  ...createAdminRoutes(root),
  ...createDeveloperRoutes(root),
  ...createSupportRoutes(root),
  ...createProfilePageRoutes(root),
  ...createPwaRoutes(root),
  ...createStorageRoutes(root),
  ...createEmuRoutes(root),
]);

function NotFoundPage() {
  const t = useT();
  return <p>{t('app.router.notFound')}</p>;
}

export { parseSearchValue, stringifySearchValue };

/** The app's route paths (`/`, `/app` and the sections), written to `dist/app/routes.json` by the app build. */
export function routePaths(): string[] {
  return Object.keys(createAppRouter().routesByPath).sort();
}

export const createAppRouter = (history?: RouterHistory) =>
  createRouter({
    routeTree,
    history,
    pathParamsAllowedCharacters: ['@'],
    defaultNotFoundComponent: NotFoundPage,
    parseSearch: parseSearchWith(parseSearchValue),
    stringifySearch: stringifySearchWith(stringifySearchValue, parseSearchValue),
  });

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
