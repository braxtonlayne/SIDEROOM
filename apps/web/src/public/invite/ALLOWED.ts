import type { AllowList } from '../allowed';
import { ALLOWED as LANDING } from '../landing/ALLOWED';

/** The invite surface: the landing's list plus the invite, template and no-invitation pages. */
export const ALLOWED: AllowList = [
  ...LANDING,
  'public/invite/',
  'routes/entrance/ServerInvite',
  'routes/entrance/ServerTemplate',
  'routes/entrance/Static',
];
