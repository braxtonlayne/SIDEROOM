import type { AllowList } from '../allowed';

/**
 * The guest-play surface (`/play/<token>`, ARC-GST-002, D-385, D-397): the minimal runner and the shared public
 * scaffolding, NOT the landing's list. It has no API client, no entrance copy, no sign-in helpers and no part of the
 * app; its single REST call and its socket are in `public/guest-play/session.ts`. Packages outside `src` (React, the
 * router, `@sideroom/ui`, `@sideroom/embedded-app-sdk`) are not app code.
 */
export const ALLOWED: AllowList = [
  // The surface itself and the shared scaffolding every public surface mounts through.
  'public/surface',
  'public/allowed',
  'public/guest-play/',
  'public/stubs/enRest',
  // The scaffolding's chrome (the "nothing here" page) and its small helpers.
  'components/EntranceShell',
  'components/SkipLink',
  'components/AppUiStrings',
  'components/icons',
  'components/entrance.css',
  'lib/useNoindex',
  'lib/searchCodec',
  // Core catalog only (entrance, a11y, pwa) plus the runner's own fragment.
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
  'i18n/messages/en/p43-guest-runner-2',
  'a11y/announcer',
  'pwa/register',
  'pwa/PublicUpdateToast',
  'pwa/pwa.css',
  'pwa/state',
  'styles/',
  // Workspace packages the runner imports (listed, as the profile surface lists its package): the theme provider
  // and button, and the SDK's protocol types and base64 helpers (no host, no OAuth; the guest bridge is ours).
  '@sideroom/ui',
  '@sideroom/embedded-app-sdk',
];
