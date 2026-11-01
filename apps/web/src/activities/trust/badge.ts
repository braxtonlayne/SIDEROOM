// What a build's badge says (ARC-SBX-001). Pure: the server decides the headline (crates/sr-activities/src/
// trust.rs `headline`), this turns it into the words, the tone and the extra facts the UI shows, so the Arcade
// page and the game's own details render the same badge from one description.
import type { MessageKey } from '../../i18n';
import type { BadgeKind, TrustView } from './types';

export type BadgeTone = 'featured' | 'signed' | 'warning' | 'plain';

export interface BadgeModel {
  kind: BadgeKind;
  tone: BadgeTone;
  /** The catalog key of the label, with its variables (the unverified-update text carries the counts). */
  labelKey: MessageKey;
  labelValues: Record<string, string | number>;
  /** The catalog key of the longer explanation shown as a tooltip or below the badge. */
  hintKey: MessageKey;
  hintValues: Record<string, string | number>;
}

/** The bracketed tag form used in plain text and logs: `[SWARM-SAFETY-SIGNED]`, `[OPERATOR-FEATURED]`. */
export function badgeTag(kind: BadgeKind): string | null {
  if (kind === 'operator_featured') return '[OPERATOR-FEATURED]';
  if (kind === 'swarm_safety_signed') return '[SWARM-SAFETY-SIGNED]';
  return null;
}

/** The model of one badge kind for a build. */
export function badgeModel(kind: BadgeKind, trust: TrustView): BadgeModel {
  const hash = trust.code_sha256?.slice(0, 8) ?? '';
  switch (kind) {
    case 'operator_featured':
      return {
        kind,
        tone: 'featured',
        labelKey: 'activities.trust.featured',
        labelValues: {},
        hintKey: 'activities.trust.featuredHint',
        hintValues: {},
      };
    case 'swarm_safety_signed':
      return {
        kind,
        tone: 'signed',
        labelKey: 'activities.trust.signed',
        labelValues: {},
        hintKey: 'activities.trust.signedHint',
        hintValues: { hash },
      };
    case 'unverified_update':
      return {
        kind,
        tone: 'warning',
        labelKey: 'activities.trust.unverifiedUpdate',
        labelValues: {
          n: trust.previous?.vouches ?? 0,
          previous: trust.previous?.short ?? '',
          current: trust.vouches,
        },
        hintKey: 'activities.trust.unsignedHint',
        hintValues: {},
      };
    default:
      return {
        kind: 'unsigned',
        tone: 'plain',
        labelKey: 'activities.trust.unsigned',
        labelValues: {},
        hintKey: 'activities.trust.unsignedHint',
        hintValues: {},
      };
  }
}

/** Every badge that applies, strongest first (the server's `badges`, with `headline` as a fallback). */
export function badgeModels(trust: TrustView): BadgeModel[] {
  const kinds = trust.badges.length ? trust.badges : [trust.headline];
  return kinds.map((k) => badgeModel(k, trust));
}

/**
 * The vouch control's state: `vouched` (withdraw), `available` (vouch), or why not. A build without a hash
 * (no reviewed code) cannot be vouched for.
 */
export type VouchState = 'vouched' | 'available' | 'not-eligible' | 'no-build';

export function vouchState(trust: TrustView): VouchState {
  if (!trust.code_sha256) return 'no-build';
  if (trust.vouched_by_me) return 'vouched';
  return trust.can_vouch ? 'available' : 'not-eligible';
}
