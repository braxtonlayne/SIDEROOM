// Wire types of the build-trust routes (ARC-SBX-001; crates/sr-activities/src/trust.rs). Ids are strings.

/** The label of a build. `operator_featured` and `swarm_safety_signed` both mean the current code is signed. */
export type BadgeKind =
  'operator_featured' | 'swarm_safety_signed' | 'unverified_update' | 'unsigned';

export interface PreviousBuild {
  code_sha256: string;
  /** The first 8 characters of the hash. */
  short: string;
  vouches: number;
  signed: boolean;
}

export interface SignatureView {
  verify_key: string;
  signature: string;
  signed_at_ms: number;
  audit_version: number;
}

export interface TrustView {
  application_id: string;
  code_sha256: string | null;
  headline: BadgeKind;
  badges: BadgeKind[];
  signature: SignatureView | null;
  featured: boolean;
  vouches: number;
  vouched_by_me: boolean;
  can_vouch: boolean;
  previous: PreviousBuild | null;
}
