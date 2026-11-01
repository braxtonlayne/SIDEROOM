import { request as pwRequest, type APIRequestContext } from '@playwright/test';
import { PUBLIC_URL } from './cli';
import { apiPatch, bootstrapState, loginApi } from './fixtures';

/**
 * STU-NAV-001 (wave 41, D-400/D-404): the storefront surfaces are OFF by default and their routes answer 404. A spec
 * that exercises them (submissions, the review queue, Movies/Series/Anime, trending) switches them on with the Owner's
 * session in `beforeAll` and off again in `afterAll`, so the rest of the run sees the defaults.
 */
export const STUDY_SWITCHES = [
  'platform.external_metadata',
  'platform.storefront_categories',
  'platform.global_trending',
  'platform.requests_board',
  'platform.ingest_queues',
] as const;

export async function setStudySurfacesWith(
  owner: APIRequestContext,
  enabled: boolean,
): Promise<void> {
  for (const id of STUDY_SWITCHES) {
    await apiPatch(owner, `/admin/features/${id}`, {
      enabled,
      reason: enabled ? 'e2e: storefront spec' : 'e2e: storefront spec done',
    });
  }
}

/** Switch the five storefront surfaces on or off as the bootstrap Owner (worker-safe: no test fixture needed). */
export async function setStudySurfaces(enabled: boolean): Promise<void> {
  const ctx = await pwRequest.newContext({ baseURL: PUBLIC_URL });
  try {
    await loginApi(ctx, bootstrapState().owner);
    await setStudySurfacesWith(ctx, enabled);
  } finally {
    await ctx.dispose();
  }
}

/** `beforeAll`/`afterAll` pair for a spec file that needs the storefront surfaces. */
export function withStudySurfacesOn(t: {
  beforeAll: (fn: () => Promise<void>) => void;
  afterAll: (fn: () => Promise<void>) => void;
}): void {
  t.beforeAll(() => setStudySurfaces(true));
  t.afterAll(() => setStudySurfaces(false));
}
