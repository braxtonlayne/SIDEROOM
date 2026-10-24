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
import { Help, HouseNotes, NoInvitation } from './routes/entrance/Static';
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
  r('/help', Help),
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

/**
 * Search values are JSON-parsed like the router's default, except integers a JS number cannot hold exactly:
 * a snowflake such as `?submission=107586859906891776` stays a string. The default turned it into a number and
 * rounded it (…891780), so staff card links (G-68) opened a submission that does not exist.
 */
const isUnsafeInteger = (raw: string) => /^-?\d+$/.test(raw) && !Number.isSafeInteger(Number(raw));

export function parseSearchValue(raw: string): unknown {
  if (isUnsafeInteger(raw)) return raw;
  return JSON.parse(raw);
}

/** The inverse: such an id is written back as bare digits, not as a quoted JSON string. */
export function stringifySearchValue(value: unknown): string {
  if (typeof value === 'string' && isUnsafeInteger(value)) return value;
  return JSON.stringify(value);
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
