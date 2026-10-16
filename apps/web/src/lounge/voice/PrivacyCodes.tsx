/**
 * VOX-E2E-007: the E2EE shield and its popover (spec section 5.5). Each participant's privacy
 * code (six groups of five digits, from sr-e2ee) is listed; people compare codes out of band
 * and mark a participant verified. A verification is stored per identity key on this device.
 *
 * Not mounted in the call UI yet: codes need the MLS epoch authenticator (VOX-E2E-001), and
 * `voice.e2ee` is off by default.
 */
import { useState } from 'react';
import { Icon, Popover } from '@sideroom/ui';
import { useT } from '../../i18n';
import type { VerifiedStore } from '../../voice/e2ee/verified';
import './voice-p14-e2ee.css';

export interface PrivacyParticipant {
  userId: string;
  name: string;
  /** "01234 56789 ..." for the current epoch. */
  code: string;
  /** Identity key fingerprint (hex), what a verification is tied to. */
  keyFingerprint: string;
  self?: boolean;
}

export interface PrivacyCodesProps {
  participants: PrivacyParticipant[];
  store: VerifiedStore;
}

export function PrivacyCodes({ participants, store }: PrivacyCodesProps) {
  const t = useT();
  // Re-render after a toggle; the store is the source of truth.
  const [, setTick] = useState(0);
  const others = participants.filter((p) => !p.self);
  const allVerified =
    others.length > 0 && others.every((p) => store.isVerified(p.userId, p.keyFingerprint));
  const label = allVerified ? t('lounge.privacyCodes.allVerified') : t('lounge.privacyCodes.title');
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
        >
          <Icon name="shield" size={20} />
        </button>
      }
    >
      <p className="vc-e2ee-intro">{t('lounge.privacyCodes.intro')}</p>
      <ul className="vc-e2ee-list">
        {participants.map((p) => {
          const verified = !p.self && store.isVerified(p.userId, p.keyFingerprint);
          return (
            <li key={p.userId} className="vc-e2ee-row" data-verified={verified || undefined}>
              <span className="vc-e2ee-name">
                {p.self ? t('lounge.privacyCodes.you', { name: p.name }) : p.name}
              </span>
              <code
                className="vc-e2ee-code"
                aria-label={t('lounge.privacyCodes.codeOf', { name: p.name })}
              >
                {p.code}
              </code>
              {!p.self && (
                <button
                  type="button"
                  className="vc-e2ee-verify"
                  aria-pressed={verified}
                  onClick={() => {
                    store.setVerified(p.userId, p.keyFingerprint, !verified);
                    setTick((n) => n + 1);
                  }}
                >
                  {verified
                    ? t('lounge.privacyCodes.verified')
                    : t('lounge.privacyCodes.markVerified')}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </Popover>
  );
}
