// The seam between the Arcade cards and the trust badges of arcade-sandbox (ARC-SBX-001, wave 42-B): code signing
// (`[SWARM-SAFETY-SIGNED]`), the Operator's `[OPERATOR-FEATURED]` and hash-pinned vouches. Cards ask this provider
// for badges and actions by build hash; the default knows none, so the Arcade works without the sandbox wave.
import { createContext, useContext, type ReactNode } from 'react';

/** What a card tells the provider about the build it shows. */
export interface CardRef {
  applicationId: string;
  name: string;
  /** SHA-256 of the reviewed bundle. */
  bundleSha256: string;
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
  if (badges.length === 0 && !trust.actions) return null;
  return (
    <div className="arc-trust">
      {badges.map((b) => (
        <span key={b.id} className={`arc-trust-badge arc-trust-${b.tone}`} title={b.title}>
          {b.label}
        </span>
      ))}
      {trust.actions?.(card)}
    </div>
  );
}
