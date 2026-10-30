import type { RouterHistory } from '@tanstack/react-router';
import surfaces from '../../public-surfaces.json';
import { createAuthRouter } from '../public/auth/router';
import { createInviteRouter } from '../public/invite/router';
import { createLandingRouter } from '../public/landing/router';

const toRe = (p: string) =>
  new RegExp(`^${p.replace(/[.+?^${}()|[\]]/g, '[$&]').replace(/\*/g, '.*')}$`);
const makers: Record<string, (h?: RouterHistory) => ReturnType<typeof createLandingRouter>> = {
  landing: createLandingRouter,
  auth: createAuthRouter,
  invite: createInviteRouter,
};

/** The router of the public surface that serves `path` (the table of public-surfaces.json decides). */
export function createPublicRouter(path: string, history?: RouterHistory) {
  const bare = path.split('?')[0]!;
  const hit = surfaces.surfaces.find((s) => s.paths.some((p) => toRe(p).test(bare)));
  const make = hit && makers[hit.surface];
  if (!make) throw new Error(`no public surface serves ${path}`);
  return make(history);
}
