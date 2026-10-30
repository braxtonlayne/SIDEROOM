import { createRoute, type RouterHistory } from '@tanstack/react-router';
import { DiscordCallback } from '../../routes/entrance/DiscordCallback';
import { EmailChangeCancel } from '../../routes/entrance/EmailChangeCancel';
import { Login } from '../../routes/entrance/Login';
import { LoginLink } from '../../routes/entrance/LoginLink';
import { Forgot, Reset } from '../../routes/entrance/PasswordReset';
import { Register } from '../../routes/entrance/Register';
import { TwoFactor } from '../../routes/entrance/TwoFactor';
import { VerifyEmail } from '../../routes/entrance/VerifyEmail';
import { createSurfaceRouter } from '../surface';

const validateLoginSearch = (s: Record<string, unknown>): { next?: string; error?: string } => ({
  ...(typeof s.next === 'string' ? { next: s.next } : {}),
  ...(typeof s.error === 'string' ? { error: s.error } : {}),
});
const validateToken = (s: Record<string, unknown>): { token?: string } =>
  typeof s.token === 'string' ? { token: s.token } : {};

/** The auth surface: sign-in, 2FA, login link, register, forgot/reset, verify, email-change cancel, Discord. */
export const createAuthRouter = (history?: RouterHistory) =>
  createSurfaceRouter(
    (root, route) => [
      createRoute({
        getParentRoute: () => root,
        path: '/login',
        component: Login,
        validateSearch: validateLoginSearch,
      }),
      createRoute({
        getParentRoute: () => root,
        path: '/login/2fa',
        component: TwoFactor,
        validateSearch: validateLoginSearch,
      }),
      createRoute({
        getParentRoute: () => root,
        path: '/login/link',
        component: LoginLink,
        validateSearch: validateToken,
      }),
      createRoute({
        getParentRoute: () => root,
        path: '/email-change/cancel',
        component: EmailChangeCancel,
        validateSearch: validateToken,
      }),
      route('/register', Register),
      route('/forgot', Forgot),
      route('/reset/$token', Reset),
      route('/verify-email/$token', VerifyEmail),
      route('/oauth/discord/callback', DiscordCallback),
    ],
    history,
  );
