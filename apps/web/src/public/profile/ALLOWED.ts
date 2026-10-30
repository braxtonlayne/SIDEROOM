import type { AllowList } from '../allowed';
import { ALLOWED as LANDING } from '../landing/ALLOWED';

/**
 * The profile surface (`/~handle`, PRF-PRO-001, D-385): the landing's list plus this page, its own image-address
 * helper and strings, and nothing else. The block renderer is `packages/profile-blocks` (outside `src`, a
 * package of pure components). It must never import `profilepage/`, the settings, the lounge or any app module.
 */
export const ALLOWED: AllowList = [
  ...LANDING,
  '@sideroom/profile-blocks',
  'public/profile/',
  'i18n/messages/en/p34-prf-profiles-core',
];
