// The Operator's Arcade calls (`OPERATOR`; every change is audited on the server). They go through the staff re-auth
// wrapper like every other Admin call.
import { apiFetch } from '../api/client';
import { withReauth } from '../admin/reauth';
import type { Candidate, ComingSoonCard, ComingSoonInput, FeaturedCard } from './types';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface ArcadeAdminApi {
  candidates(): Promise<Candidate[]>;
  feature(
    applicationId: string,
    input: { position?: number; blurb?: string | null },
  ): Promise<FeaturedCard>;
  unfeature(applicationId: string): Promise<void>;
  addComingSoon(input: ComingSoonInput): Promise<ComingSoonCard>;
  removeComingSoon(entryId: string): Promise<void>;
}

export const httpArcadeAdminApi: ArcadeAdminApi = {
  candidates: () => withReauth(() => apiFetch('/admin/arcade/candidates', NO_RETRY)),
  feature: (id, input) =>
    withReauth(() =>
      apiFetch(`/admin/arcade/featured/${enc(id)}`, { method: 'PUT', json: input, ...NO_RETRY }),
    ),
  unfeature: async (id) => {
    await withReauth(() =>
      apiFetch(`/admin/arcade/featured/${enc(id)}`, { method: 'DELETE', ...NO_RETRY }),
    );
  },
  addComingSoon: (input) =>
    withReauth(() =>
      apiFetch('/admin/arcade/coming-soon', { method: 'POST', json: input, ...NO_RETRY }),
    ),
  removeComingSoon: async (id) => {
    await withReauth(() =>
      apiFetch(`/admin/arcade/coming-soon/${enc(id)}`, { method: 'DELETE', ...NO_RETRY }),
    );
  },
};
