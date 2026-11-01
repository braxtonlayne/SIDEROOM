// Wire types of the Arcade routes (`crates/sr-activities/src/arcade.rs`, `crates/sr-api/src/routes/arcade.rs`).
// Ids are snowflake strings.
import type { TipLink } from './tips';

/** The two content tiers a game manifest may declare. There is no third. */
export const TIERS = ['TIER_1_ORIGINAL', 'TIER_2_REIMPLEMENTATION'] as const;
export type Tier = (typeof TIERS)[number];

export interface Eligibility {
  browser_play: boolean;
  guest_links: boolean;
  featured: boolean;
  desktop_only: boolean;
  needs_base_game_hint: boolean;
}

export type FlagStatus = 'active' | 'reserved';

/** A capability flag a manifest may declare (roadmap rule 5). */
export interface CapabilityFlag {
  name: string;
  status: FlagStatus;
  summary: string;
}

export interface FeaturedCard {
  application_id: string;
  name: string;
  description: string;
  blurb?: string | null;
  tier: Tier;
  eligibility: Eligibility;
  capabilities: string[];
  tip_links: TipLink[];
  /** SHA-256 of the reviewed bundle (the whole zip, assets included). */
  bundle_sha256: string;
  /**
   * The hash of the reviewed build's code only (ARC-SBX-001): the key the trust badges attach to, because the
   * operator's signature and the vouches are pinned to it. Null until computed for an old bundle.
   */
  code_sha256?: string | null;
  position: number;
  featured_at_ms: number;
}

export interface ComingSoonCard {
  id: string;
  title: string;
  summary: string;
  tier: Tier;
  repo_url: string;
  notify: boolean;
  created_at_ms: number;
}

export interface ArcadeFront {
  featured: FeaturedCard[];
  coming_soon: ComingSoonCard[];
  flags: CapabilityFlag[];
}

export interface Cartridge {
  id: string;
  guild_id: string;
  title: string;
  description: string;
  kind: 'game' | 'mod';
  tier: Tier;
  eligibility: Eligibility;
  source_ref: string;
  /** A wave-41 shelf object (both or neither): fetched through the per-object URL route. */
  shelf_id?: string | null;
  object_hash?: string | null;
  capabilities: string[];
  tip_links: TipLink[];
  added_by: string;
  added_at_ms: number;
}

export interface CartridgeInput {
  title: string;
  description: string;
  kind: 'game' | 'mod';
  tier: Tier;
  source_ref: string;
  shelf_id?: string;
  object_hash?: string;
  capabilities: string[];
  tip_links: TipLink[];
}

export interface Candidate {
  application_id: string;
  name: string;
  tier: Tier;
  bundle_sha256: string;
  featured: boolean;
}

export interface ComingSoonInput {
  title: string;
  summary: string;
  tier: Tier;
  repo_url: string;
}
