// Typed fetchers of the build-trust routes. Components never build URLs.
import { apiFetch } from '../../api/client';
import type { TrustView } from './types';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface TrustApi {
  get(applicationId: string): Promise<TrustView>;
  /** Vouch for the build `codeSha256` (must be the one served now). */
  vouch(applicationId: string, codeSha256: string): Promise<TrustView>;
  /** Withdraw my vouches for this game. */
  unvouch(applicationId: string): Promise<TrustView>;
}

export const httpTrustApi: TrustApi = {
  get: (id) => apiFetch(`/activities/${enc(id)}/trust`, NO_RETRY),
  vouch: (id, code_sha256) =>
    apiFetch(`/activities/${enc(id)}/vouch`, { method: 'PUT', json: { code_sha256 }, ...NO_RETRY }),
  unvouch: (id) => apiFetch(`/activities/${enc(id)}/vouch`, { method: 'DELETE', ...NO_RETRY }),
};
