import type { AllowList } from '../allowed';
import { ALLOWED as LANDING } from '../landing/ALLOWED';

/**
 * The profile surface (`/~handle`, PRF-PRO-001, D-385): the landing's list plus this page, the image-address
 * helper, and nothing else. The block renderer is `packages/profile-blocks` (outside `src`, an import-free
 * package of pure components). It must never import `profilepage/`, the settings, the lounge or any app module.
 */
export const ALLOWED: AllowList = [...LANDING, 'public/profile/', 'store/cdn'];
