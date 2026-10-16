/**
 * VOX-E2E-007: the E2EE shield and its popover (spec section 5.5, D-250). The call code (from
 * the current MLS epoch) is the same for everyone in the call right now and is compared live;
 * each participant's pairwise verification code stays the same across calls until one of the
 * two identity keys changes, and backs the "verified" mark kept on this device per key.
 */
import { useState } from 'react';
import { Icon, Popover } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useE2ee } from '../../voice/e2ee/store';
import { useVoiceSettings } from '../../voice/settings';
import { verifiedStore, type VerifiedStore } from '../../voice/e2ee/verified';
import { useUsers } from '../data';
import './voice-p14-e2ee.css';

export interface PrivacyParticipant {
  userId: string;
  name: string;
  /** The pairwise verification code between the viewer and this person. */
  code: string;
  /** Their identity key (hex): what a verification is tied to. */
  keyFingerprint: string;
}

export interface PrivacyCodesProps {
  /** The call code of the current epoch; null while the call is not secured yet. */
  callCode: string | null;
  participants: PrivacyParticipant[];
  store: VerifiedStore;
  /** Something went wrong in the key agreement (the call is being re-secured). */
  error?: boolean;
}

export function PrivacyCodes({ callCode, participants, store, error }: PrivacyCodesProps) {
  const t = useT();
  // Re-render after a toggle; the store is the source of truth.
  const [, setTick] = useState(0);
  const allVerified =
    participants.length > 0 &&
    participants.every((p) => store.isVerified(p.userId, p.keyFingerprint));
  const showCodes = useVoiceSettings((s) => s.settings.showPrivacyCodes);
  const label = !callCode
    ? t('lounge.privacyCodes.securing')
    : allVerified
      ? t('lounge.privacyCodes.allVerified')
      : t('lounge.privacyCodes.title');
  return (
    <Popover
      label={t('lounge.privacyCodes.title')}
      placement="top"
      className="vc-e2ee-popover"
      trigger={
        <button
          type="button"
          className="lng-icon-btn vc-e2ee-shield"
          aria-label={label}
          data-verified={allVerified || undefined}
          data-pending={!callCode || error || undefined}
        >
          <Icon name="shield" size={20} />
        </button>
      }
    >
      <p className="vc-e2ee-intro">{t('lounge.privacyCodes.intro')}</p>
      {!showCodes ? (
        <p className="vc-e2ee-intro">{t('lounge.privacyCodes.hidden')}</p>
      ) : callCode ? (
        <div className="vc-e2ee-call">
          <span className="vc-e2ee-name">{t('lounge.privacyCodes.callCode')}</span>
          <code className="vc-e2ee-code" aria-label={t('lounge.privacyCodes.callCode')}>
            {callCode}
          </code>
        </div>
      ) : (
        <p className="vc-e2ee-intro">{t('lounge.privacyCodes.securing')}</p>
      )}
      {showCodes && participants.length > 0 && (
        <ul className="vc-e2ee-list">
          {participants.map((p) => {
            const isVerified = store.isVerified(p.userId, p.keyFingerprint);
            return (
              <li key={p.userId} className="vc-e2ee-row" data-verified={isVerified || undefined}>
                <span className="vc-e2ee-name">{p.name}</span>
                <code
                  className="vc-e2ee-code"
                  aria-label={t('lounge.privacyCodes.codeOf', { name: p.name })}
                >
                  {p.code}
                </code>
                <button
                  type="button"
                  className="vc-e2ee-verify"
                  aria-pressed={isVerified}
                  onClick={() => {
                    store.setVerified(p.userId, p.keyFingerprint, !isVerified);
                    setTick((n) => n + 1);
                  }}
                >
                  {isVerified
                    ? t('lounge.privacyCodes.verified')
                    : t('lounge.privacyCodes.markVerified')}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </Popover>
  );
}

const deviceVerified = verifiedStore();

/** The shield for the viewer's current call; nothing when the call is not end-to-end encrypted. */
export function E2eeShield() {
  const t = useT();
  const view = useE2ee();
  const users = useUsers();
  if (!view.active || !view.call) return null;
  const participants = view.call.members
    .filter((m) => view.pairwise[m.userId])
    .map((m) => ({
      userId: m.userId,
      name: users[m.userId]?.displayName ?? t('lounge.voiceChannel.unknown'),
      code: view.pairwise[m.userId]!,
      keyFingerprint: m.identityKey,
    }));
  return (
    <PrivacyCodes
      callCode={view.call.status === 'secure' ? view.call.callCode : null}
      participants={participants}
      store={deviceVerified}
      error={view.call.status === 'error'}
    />
  );
}
