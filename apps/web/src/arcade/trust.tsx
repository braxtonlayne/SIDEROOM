// The seam between the Arcade cards and the trust badges of arcade-sandbox (ARC-SBX-001, wave 42-B): code signing
// (`[SWARM-SAFETY-SIGNED]`), the Operator's `[OPERATOR-FEATURED]` and hash-pinned vouches. Cards ask this provider
// for badges and actions; the default knows none, so the Arcade renders without it. The page wires the sandbox's
// `BuildTrust` in through `Card` (see `sandboxTrust.tsx`).
//
// Key: a card's trust is keyed on `code_sha256`, the hash of the build's code only, because that is what the
// operator signs and what a vouch is pinned to. `bundle_sha256` (the whole zip) changes with art and audio too,
// which would discard a valid signature and every vouch for nothing.
import { createContext, useContext, type ComponentType, type ReactNode } from 'react';

/** What a card tells the provider about the build it shows. */
export interface CardRef {
  applicationId: string;
  name: string;
  /** The hash of the reviewed build's code (`code_sha256`); null until the server computed it. */
  codeSha256: string | null;
}

export interface TrustBadge {
  id: string;
  /** Already translated by the provider. */
  label: string;
  tone: 'good' | 'warning' | 'info';
  title?: string;
}

export interface TrustSource {
  badges(card: CardRef): TrustBadge[];
  /** Extra controls on a card, such as a "Vouch for Build" button. */
  actions?(card: CardRef): ReactNode;
  /** A component that loads and renders the card's trust itself (badges and the vouch control). */
  Card?: ComponentType<{ card: CardRef }>;
}

const NONE: TrustSource = { badges: () => [] };

const Ctx = createContext<TrustSource>(NONE);

export function TrustProvider({ source, children }: { source: TrustSource; children: ReactNode }) {
  return <Ctx.Provider value={source}>{children}</Ctx.Provider>;
}

export const useTrust = (): TrustSource => useContext(Ctx);

export function TrustBadges({ card }: { card: CardRef }) {
  const trust = useTrust();
  const badges = trust.badges(card);
  const { Card } = trust;
  if (badges.length === 0 && !trust.actions && !Card) return null;
  return (
    <div className="arc-trust">
      {badges.map((b) => (
        <span key={b.id} className={`arc-trust-badge arc-trust-${b.tone}`} title={b.title}>
          {b.label}
        </span>
      ))}
      {trust.actions?.(card)}
      {/* A new code hash is a new build: remount so its badge and vouch state load afresh. */}
      {Card && <Card key={`${card.applicationId}:${card.codeSha256 ?? ''}`} card={card} />}
    </div>
  );
}
