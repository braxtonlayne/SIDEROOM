import type { AllowList } from '../allowed';

/** Everything the landing surface (`/`, `/join/$code`) may contain. */
export const ALLOWED: AllowList = [
  // The surface itself and the shared scaffolding every public surface mounts through.
  'public/surface',
  'public/allowed',
  'public/landing/',
  'public/stubs/enRest',
  // The page, its chrome and its calls.
  'routes/entrance/Landing',
  'routes/entrance/entranceCopy',
  'components/Door',
  'components/EntranceShell',
  'components/SkipLink',
  'components/icons',
  'components/entrance.css',
  'api/client',
  'api/entrance',
  'lib/inviteCode',
  'lib/compliancePublic',
  'lib/useNoindex',
  'lib/enterApp',
  'lib/entranceTheme',
  'lib/searchCodec',
  'lib/useRedirectIfLoggedIn',
  // Core catalog only: entrance, a11y and pwa. The rest of the catalog never reaches a public surface.
  'i18n/index',
  'i18n/react',
  'i18n/runtime',
  'i18n/icu',
  'i18n/pseudo',
  'i18n/messages/en',
  'i18n/messages/store',
  'i18n/messages/merge',
  'i18n/messages/en/entrance',
  'i18n/messages/en/a11y',
  'i18n/messages/en/pwa',
  'a11y/announcer',
  'pwa/register',
  'pwa/state',
  'styles/',
];
