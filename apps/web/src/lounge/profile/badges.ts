/**
 * Badges (LNG-PRF-008). Platform badges come from the profile (`badges[]`, Discord's shape); two more are derived
 * on the client from what it already knows: the owner of the server the profile is shown in, and the person's
 * server tag. Tiered badges (`patron-gold`, `contributor-2`) share one icon and carry the tier in the label.
 */
import type { IconName } from '@sideroom/ui';
import { t } from '../../i18n';
import type { WireBadge } from './useProfile';

export type BadgeKind =
  | 'staff'
  | 'owner'
  | 'patron'
  | 'contributor'
  | 'early-member'
  | 'app-developer'
  | 'server-tag'
  | 'other';

export interface BadgeView {
  key: string;
  kind: BadgeKind;
  label: string;
  /** Tooltip text; the label when the server sent none. */
  description: string;
  icon: IconName;
  /** Server-tag text for the chip. */
  text?: string;
}

const ICON: Record<BadgeKind, IconName> = {
  staff: 'shield',
  owner: 'star',
  patron: 'heart',
  contributor: 'sparkle',
  'early-member': 'clock',
  'app-developer': 'settings',
  'server-tag': 'users',
  other: 'info',
};

const TIERED = /^(patron|contributor)[-_:.]?(.*)$/;

function kindOf(id: string): { kind: BadgeKind; tier: string } {
  const lower = id.toLowerCase().replace(/_/g, '-');
  const m = TIERED.exec(lower);
  if (m) return { kind: m[1] as 'patron' | 'contributor', tier: m[2]?.replace(/-/g, ' ') ?? '' };
  if (lower === 'staff' || lower === 'platform-staff') return { kind: 'staff', tier: '' };
  if (lower === 'early-member' || lower === 'early-supporter')
    return { kind: 'early-member', tier: '' };
  if (lower === 'app-developer' || lower === 'developer')
    return { kind: 'app-developer', tier: '' };
  return { kind: 'other', tier: '' };
}

function labelOf(kind: BadgeKind, tier: string, fallback: string): string {
  const base = {
    staff: () => t('lounge.badges.staff'),
    owner: () => t('lounge.badges.owner'),
    patron: () => t('lounge.badges.patron'),
    contributor: () => t('lounge.badges.contributor'),
    'early-member': () => t('lounge.badges.earlyMember'),
    'app-developer': () => t('lounge.badges.appDeveloper'),
    'server-tag': () => t('lounge.badges.serverTag'),
    other: () => fallback,
  }[kind]();
  if (!tier) return base;
  const nice = tier.replace(/^./, (c) => c.toUpperCase());
  return t('lounge.badges.tiered', { badge: base, tier: nice });
}

export interface DerivedBadgeInput {
  /** The person owns the server the profile is shown in. */
  isServerOwner?: boolean;
  /** Server-tag text (the chip shown next to the name). */
  serverTag?: string | null | undefined;
}

/** All badges to show, in display order: owner, platform badges as sent, then the server tag. */
export function badgeViews(
  wire: readonly WireBadge[] | null | undefined,
  derived: DerivedBadgeInput = {},
): BadgeView[] {
  const out: BadgeView[] = [];
  if (derived.isServerOwner)
    out.push({
      key: 'owner',
      kind: 'owner',
      label: labelOf('owner', '', ''),
      description: t('lounge.badges.ownerHint'),
      icon: ICON.owner,
    });
  const seen = new Set<string>();
  for (const b of wire ?? []) {
    if (!b?.id || seen.has(b.id)) continue;
    seen.add(b.id);
    const { kind, tier } = kindOf(b.id);
    const label = labelOf(kind, tier, b.description || b.id);
    out.push({
      key: `w-${b.id}`,
      kind,
      label,
      description: b.description || label,
      icon: ICON[kind],
    });
  }
  if (derived.serverTag)
    out.push({
      key: 'server-tag',
      kind: 'server-tag',
      label: labelOf('server-tag', '', ''),
      description: t('lounge.badges.serverTagHint', { tag: derived.serverTag }),
      icon: ICON['server-tag'],
      text: derived.serverTag,
    });
  return out;
}
