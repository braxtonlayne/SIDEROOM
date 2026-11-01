// The badge(s) of a game's current build and the "Vouch for Build" control (ARC-SBX-001). The Arcade page
// (ARC-ROOM-001) renders these; they take a `TrustView` (or a `useBuildTrust` state) and know nothing about
// where they are shown.
import { Button } from '@sideroom/ui';
import { useT } from '../../i18n';
import { badgeModels, vouchState } from './badge';
import type { BuildTrustState } from './useBuildTrust';
import type { TrustView } from './types';
import './trust.css';

/** Every badge that applies to the build, as pills. */
export function BuildBadges({ trust }: { trust: TrustView }) {
  const t = useT();
  return (
    <span className="trust-badges">
      {badgeModels(trust).map((m) => (
        <span
          key={m.kind}
          className="trust-badge"
          data-tone={m.tone}
          data-kind={m.kind}
          title={t(m.hintKey, m.hintValues)}
        >
          {t(m.labelKey, m.labelValues)}
        </span>
      ))}
    </span>
  );
}

/** The vouch count and the Vouch for Build / Withdraw control. */
export function VouchControl({ state }: { state: BuildTrustState }) {
  const t = useT();
  const { trust, busy, actionFailed } = state;
  if (!trust) return null;
  const v = vouchState(trust);
  return (
    <div className="trust-vouch">
      <span>{t('activities.trust.vouchCount', { n: trust.vouches })}</span>
      {v === 'available' && (
        <Button size="sm" disabled={busy} onClick={() => void state.vouch()}>
          {t('activities.trust.vouch')}
        </Button>
      )}
      {v === 'vouched' && (
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void state.unvouch()}>
          {t('activities.trust.unvouch')}
        </Button>
      )}
      {v === 'not-eligible' && (
        <p className="trust-vouch-hint">{t('activities.trust.notEligible')}</p>
      )}
      {v === 'available' && <p className="trust-vouch-hint">{t('activities.trust.vouchHint')}</p>}
      {actionFailed && <p role="alert">{t('activities.trust.vouchFailed')}</p>}
    </div>
  );
}

/** Badge and vouch control for one game. */
export function BuildTrust({ state }: { state: BuildTrustState }) {
  const t = useT();
  if (state.failed) return <p className="trust-vouch-hint">{t('activities.trust.loadFailed')}</p>;
  if (!state.trust) return null;
  return (
    <div>
      <BuildBadges trust={state.trust} />
      <VouchControl state={state} />
    </div>
  );
}
