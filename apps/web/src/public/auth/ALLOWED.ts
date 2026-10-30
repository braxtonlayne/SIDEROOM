import type { AllowList } from '../allowed';
import { ALLOWED as LANDING } from '../landing/ALLOWED';

/** The auth surface: the landing's list plus the sign-in pages and what only they need. */
export const ALLOWED: AllowList = [
  ...LANDING,
  'public/auth/',
  'routes/entrance/Login',
  'routes/entrance/TwoFactor',
  'routes/entrance/LoginLink',
  'routes/entrance/Register',
  'routes/entrance/PasswordReset',
  'routes/entrance/VerifyEmail',
  'routes/entrance/EmailChangeCancel',
  'routes/entrance/DiscordCallback',
  'routes/entrance/BanAppeal',
  'components/AuthSplit',
  'components/FormParts',
  'components/PasskeyButton',
  'components/SignInHelp',
  'components/VerifyEmailBanner',
  'api/passkeys',
  'lib/validation',
  'lib/mfaMethods',
  'lib/webauthn',
];
