// Typed fetchers of the profile vouch routes (MOD-VCH-001). Not the Arcade build vouches (activities/trust).
import { apiFetch } from '../../api/client';

export type VouchTier = 'citizen' | 'pillar' | 'elder';

export interface VouchSummary {
  count: number;
  tier: VouchTier;
  /** 1.0, 1.5 or 3.0: how much heavier automated penalties land on this member. */
  multiplier: number;
  pillar_min: number;
  elder_min: number;
  pillar_multiplier: number;
  elder_multiplier: number;
  viewer_vouched: boolean;
  can_vouch: boolean;
  is_self: boolean;
}

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface VouchApi {
  get(userId: string): Promise<VouchSummary>;
  vouch(userId: string): Promise<VouchSummary>;
  revoke(userId: string): Promise<void>;
}

export const httpVouchApi: VouchApi = {
  get: (id) => apiFetch(`/users/${enc(id)}/vouches`, NO_RETRY),
  vouch: (id) => apiFetch(`/users/${enc(id)}/vouch`, { method: 'PUT', ...NO_RETRY }),
  revoke: (id) => apiFetch(`/users/${enc(id)}/vouch`, { method: 'DELETE', ...NO_RETRY }),
};
