import { useId, type ReactNode } from 'react';
import { useT } from '../i18n';
import { GameArt } from './GameArt';
import { TipBadges } from './TipBadges';
import { TrustBadges, type CardRef } from './trust';
import type { CapabilityFlag, Eligibility, Tier } from './types';

/** The tier as a short label, with what it means for where the game runs. */
export function TierLine({ tier, eligibility }: { tier: Tier; eligibility: Eligibility }) {
  const t = useT();
  const label =
    tier === 'TIER_1_ORIGINAL' ? t('arcade.tier.original') : t('arcade.tier.reimplementation');
  return (
    <p className="arc-tier" aria-label={t('arcade.tier.aria', { tier: label })}>
      <span className="arc-chip">{label}</span>
      {eligibility.desktop_only && <span className="arc-chip">{t('arcade.tier.desktopOnly')}</span>}
      {eligibility.needs_base_game_hint && (
        <span className="arc-chip">{t('arcade.tier.needsBase')}</span>
      )}
    </p>
  );
}

/** A capability flag's readable name (`guest_play` reads "Guest play"); an unknown flag keeps its own name. */
export function flagLabel(t: (key: string) => string, name: string): string {
  // Catalog key segments are camelCase: `guest_play` looks up `arcade.flag.name.guestPlay`.
  if (/^[a-z][a-z0-9_]*$/.test(name)) {
    const key = `arcade.flag.name.${name.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())}`;
    const label = t(key);
    if (label !== key) return label;
  }
  const plain = name.replace(/_/g, ' ');
  return plain.charAt(0).toUpperCase() + plain.slice(1);
}

/** Capability flags a build declares; a reserved one is marked as planned. */
export function FlagChips({
  names,
  flags,
  title,
}: {
  names: readonly string[];
  flags: readonly CapabilityFlag[];
  title: string;
}) {
  const t = useT();
  if (names.length === 0) return null;
  return (
    <ul className="arc-flags" aria-label={t('arcade.flag.group', { name: title })}>
      {names.map((n) => {
        const reserved = flags.find((f) => f.name === n)?.status === 'reserved';
        return (
          <li key={n} className="arc-chip" title={flags.find((f) => f.name === n)?.summary}>
            {flagLabel(t, n)}
            {reserved && <span className="arc-chip-note"> ({t('arcade.flag.reserved')})</span>}
          </li>
        );
      })}
    </ul>
  );
}

export interface GameCardProps {
  title: string;
  description: string;
  blurb?: string | null;
  tier?: Tier;
  eligibility?: Eligibility;
  capabilities?: readonly string[];
  flags?: readonly CapabilityFlag[];
  tips?: readonly { kind: string; url: string }[];
  /** Present when the card is a build the trust badges can attach to. */
  trust?: CardRef;
  badge?: string;
  /** The cover art (ARC-UI-010): an art kind from `GameArt`; the title colours the generic paintings. */
  art?: string;
  /** The status ribbon over the art: playable now, or still in development. */
  status?: 'live' | 'preview';
  /** A short line under the description (player count, a hint). */
  meta?: ReactNode;
  /** Make a card with no control of its own reachable by Tab, so a keyboard can walk the shelf. */
  focusable?: boolean;
  /** `feature` spans the grid as a wide card with the art beside the text. */
  size?: 'normal' | 'feature';
  /** Controls and notes, laid out as the card's footer. */
  children?: ReactNode;
}

export function GameCard(p: GameCardProps) {
  const t = useT();
  const titleId = useId();
  return (
    <article
      className="arc-card"
      data-size={p.size ?? 'normal'}
      data-status={p.status}
      tabIndex={p.focusable ? 0 : undefined}
      aria-labelledby={titleId}
    >
      {p.art && (
        <div className="arc-card-art">
          <GameArt kind={p.art} seed={p.title} />
          {p.status && (
            <span className="arc-ribbon" data-status={p.status}>
              {t(p.status === 'live' ? 'arcade.status.live' : 'arcade.status.preview')}
            </span>
          )}
        </div>
      )}
      <div className="arc-card-body">
        <header className="arc-card-head">
          <h3 className="arc-card-title" id={titleId}>
            {p.title}
          </h3>
          {p.badge && <span className="arc-chip arc-chip-badge">{p.badge}</span>}
        </header>
        {p.blurb && <p className="arc-blurb">{p.blurb}</p>}
        {p.description && <p className="arc-desc">{p.description}</p>}
        {p.meta && <p className="arc-meta">{p.meta}</p>}
        {p.tier && p.eligibility && <TierLine tier={p.tier} eligibility={p.eligibility} />}
        <FlagChips names={p.capabilities ?? []} flags={p.flags ?? []} title={p.title} />
        {p.trust && <TrustBadges card={p.trust} />}
        <TipBadges links={p.tips} name={p.title} />
        {p.children && <div className="arc-card-foot">{p.children}</div>}
      </div>
    </article>
  );
}
