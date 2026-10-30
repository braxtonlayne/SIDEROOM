// PRF-PRO-001 (D-382): the member's profile privacy. Thin wrappers over apiFetch.
import { apiFetch } from '../../../api/client';
import type { ProfilePage } from '../../../profilepage/types';

export const TIERS = ['public', 'members', 'mutual_servers', 'friends', 'only_me'] as const;
export type Tier = (typeof TIERS)[number];

export const PRESETS = ['public', 'members', 'friends', 'private'] as const;
export type Preset = (typeof PRESETS)[number] | 'custom';

/** The block audiences, in the order the panel lists them. */
export const BLOCK_FIELDS = [
  'links_tier',
  'bio_tier',
  'media_tier',
  'top8_tier',
  'now_playing_tier',
  'guestbook_tier',
  'ring_tier',
] as const;
export type BlockField = (typeof BLOCK_FIELDS)[number];

export interface Policy {
  page_tier: Tier;
  links_tier: Tier;
  top8_tier: Tier;
  now_playing_tier: Tier;
  bio_tier: Tier;
  guestbook_tier: Tier;
  media_tier: Tier;
  ring_tier: Tier;
  guestbook_accepts_public: boolean;
}

export interface PrivacyView {
  preset: Preset;
  stored: Policy;
  effective: Policy;
  public_possible: boolean;
}

export type PrivacyUpdate = { preset: Preset } | Partial<Policy>;

export type ViewAs = 'public' | 'member' | 'shares_server' | 'friend';

const ro = { retries: 0 } as const;

export const getPrivacy = () => apiFetch<PrivacyView>('/users/@me/profile-privacy', ro);
export const putPrivacy = (update: PrivacyUpdate) =>
  apiFetch<PrivacyView>('/users/@me/profile-privacy', { method: 'PUT', json: update, ...ro });
export const previewAs = (as: ViewAs) =>
  apiFetch<ProfilePage>(`/users/@me/profile-page/preview?as=${as}`, ro);

/** The tiers a block may take under a page tier: never looser than the page. */
export const allowedTiers = (page: Tier): Tier[] => TIERS.slice(TIERS.indexOf(page));
