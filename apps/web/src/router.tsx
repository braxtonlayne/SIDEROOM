import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  useRouterState,
  type RouterHistory,
} from '@tanstack/react-router';
import { lazy, Suspense } from 'react';
import { useT } from './i18n';
import { CatalogGate } from './i18n/CatalogGate';
import { isCorePath } from './i18n/corePaths';
import { RouteFallback } from './lib/lazyRoute';
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
import { Landing } from './routes/entrance/Landing';
import { Login } from './routes/entrance/Login';
import { LoginLink } from './routes/entrance/LoginLink';
import { Forgot, Reset } from './routes/entrance/PasswordReset';
import { Register } from './routes/entrance/Register';
import { ServerInvite } from './routes/entrance/ServerInvite';
import { HouseNotes, NoInvitation } from './routes/entrance/Static';
import { TwoFactor } from './routes/entrance/TwoFactor';
import { DiscordCallback } from './routes/entrance/DiscordCallback';
import { VerifyEmail } from './routes/entrance/VerifyEmail';
import { EmailChangeCancel } from './routes/entrance/EmailChangeCancel';
import { ServerTemplate } from './routes/entrance/ServerTemplate';

// The re-auth dialog (ADM-STF-009) lives at the root so staff-card buttons in the Lounge can open the
// second-factor window too; it is its own chunk and renders nothing until asked.
const ReauthHost = lazy(() =>
  import('./admin/ReauthHost').then((m) => ({ default: m.ReauthHost })),
);
function RootLayout() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <>
      {/* Entrance routes paint from the core catalog; every other route waits for the full one. */}
      <CatalogGate bypass={isCorePath(pathname)} fallback={<RouteFallback />}>
        <Outlet />
      </CatalogGate>
      <CatalogGate>
        <Suspense fallback={null}>
          <ReauthHost />
        </Suspense>
      </CatalogGate>
    </>
  );
}
const root = createRootRoute({ component: RootLayout });
const r = <P extends string>(path: P, component: () => React.JSX.Element | null) =>
  createRoute({ getParentRoute: () => root, path, component });

const validateLoginSearch = (s: Record<string, unknown>): { next?: string; error?: string } => ({
  ...(typeof s.next === 'string' ? { next: s.next } : {}),
  ...(typeof s.error === 'string' ? { error: s.error } : {}),
});

const loginRoute = createRoute({
  getParentRoute: () => root,
  path: '/login',
  component: Login,
  validateSearch: validateLoginSearch,
});
const twoFactorRoute = createRoute({
  getParentRoute: () => root,
  path: '/login/2fa',
  component: TwoFactor,
  validateSearch: validateLoginSearch,
});
const loginLinkRoute = createRoute({
  getParentRoute: () => root,
  path: '/login/link',
  component: LoginLink,
  validateSearch: (s: Record<string, unknown>): { token?: string } =>
    typeof s.token === 'string' ? { token: s.token } : {},
});
const emailChangeCancelRoute = createRoute({
  getParentRoute: () => root,
  path: '/email-change/cancel',
  component: EmailChangeCancel,
  validateSearch: (s: Record<string, unknown>): { token?: string } =>
    typeof s.token === 'string' ? { token: s.token } : {},
});
const joinRoute = createRoute({
  getParentRoute: () => root,
  path: '/join/$code',
  component: function Join() {
    const { code } = joinRoute.useParams();
    return <Landing initialCode={code} />;
  },
});

const routeTree = root.addChildren([
  r('/', () => <Landing />),
  loginRoute,
  twoFactorRoute,
  loginLinkRoute,
  emailChangeCancelRoute,
  joinRoute,
  r('/register', Register),
  r('/forgot', Forgot),
  r('/reset/$token', Reset),
  r('/verify-email/$token', VerifyEmail),
  r('/oauth/discord/callback', DiscordCallback),
  r('/no-invitation', NoInvitation),
  r('/house-notes', HouseNotes),
  r('/invite/$code', ServerInvite),
  r('/template/$code', ServerTemplate),
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
]);

function NotFoundPage() {
  const t = useT();
  return <p>{t('app.router.notFound')}</p>;
}

export const createAppRouter = (history?: RouterHistory) =>
  createRouter({
    routeTree,
    history,
    pathParamsAllowedCharacters: ['@'],
    defaultNotFoundComponent: NotFoundPage,
  });

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
