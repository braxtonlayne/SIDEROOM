// The Arcade's trust source (wave 42 integration): the sandbox's build trust (ARC-SBX-001) on every Arcade card,
// through the card seam of ARC-ROOM-001. Each card loads `GET /activities/{id}/trust` and shows the badges
// (OPERATOR-FEATURED, SWARM-SAFETY-SIGNED, UNVERIFIED UPDATE, unsigned) and the "Vouch for Build" control.
import { BuildTrust, useBuildTrust } from '../activities/trust';
import type { CardRef, TrustSource } from './trust';

function SandboxCardTrust({ card }: { card: CardRef }) {
  const state = useBuildTrust(card.applicationId);
  return <BuildTrust state={state} />;
}

export const sandboxTrust: TrustSource = { badges: () => [], Card: SandboxCardTrust };
