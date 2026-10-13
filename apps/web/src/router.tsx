import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
} from '@tanstack/react-router';
import { createLoungeRoutes } from './routes/lounge';
import { createUserSettingsRoutes } from './settings/user/routes';
import { createServerSettingsRoutes } from './routes/settingsServer';
import * as P from './routes/pages';
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
  joinRoute,
  r('/register', Register),
  r('/forgot', Forgot),
  r('/reset/$token', Reset),
  r('/verify-email/$token', VerifyEmail),
  r('/oauth/discord/callback', DiscordCallback),
  r('/no-invitation', NoInvitation),
  r('/house-notes', HouseNotes),
  r('/invite/$code', ServerInvite),
  createLoungeRoutes(root),
  ...createUserSettingsRoutes(root),
  ...createServerSettingsRoutes(root),
  r('/cinema', P.Cinema),
  r('/library', P.Library),
  r('/admin', P.Admin),
  r('/developers', P.Developers),
  r('/support', P.Support),
]);

export const createAppRouter = (history?: RouterHistory) =>
  createRouter({
    routeTree,
    history,
    pathParamsAllowedCharacters: ['@'],
    defaultNotFoundComponent: () => <p>Page not found.</p>,
  });

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
