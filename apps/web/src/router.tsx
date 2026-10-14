import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
} from '@tanstack/react-router';
import { useT } from './i18n';
import { createCinemaRoutes } from './cinema/routes';
import { createLibraryRoutes } from './library/routes';
import { createLoungeRoutes } from './routes/lounge';
import { createUserSettingsRoutes } from './settings/user/routes';
import { createServerSettingsRoutes } from './routes/settingsServer';
import { createAdminRoutes } from './admin/routes';
import { createDeveloperRoutes } from './developers/routes';
import { createSupportRoutes } from './support/routes';
import { Landing } from './routes/entrance/Landing';
import { Login } from './routes/entrance/Login';
import { LoginLink } from './routes/entrance/LoginLink';
import { Forgot, Reset } from './routes/entrance/PasswordReset';
import { Register } from './routes/entrance/Register';
import { ServerInvite } from './routes/entrance/ServerInvite';
import { HouseNotes, NoInvitation, Support } from './routes/entrance/Static';
import { TwoFactor } from './routes/entrance/TwoFactor';
import { DiscordCallback } from './routes/entrance/DiscordCallback';
import { VerifyEmail } from './routes/entrance/VerifyEmail';
import { EmailChangeCancel } from './routes/entrance/EmailChangeCancel';
import { ServerTemplate } from './routes/entrance/ServerTemplate';

const root = createRootRoute({ component: Outlet });
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
  r('/support', Support),
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
