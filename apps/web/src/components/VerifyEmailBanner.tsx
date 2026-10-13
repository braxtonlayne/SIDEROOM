import { OutlineButton } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { resendVerification } from '../api/entrance';
import { useT } from '../i18n/react';

type Sent = 'idle' | 'sending' | 'sent' | 'wait' | 'error';

/**
 * ENT-UI-020: "verify your email" banner for signed-in members whose address is not confirmed yet.
 * The caller decides `verified` (from the account); the banner renders nothing once it is true.
 */
export function VerifyEmailBanner({ verified }: { verified: boolean }) {
  const t = useT();
  const [state, setState] = useState<Sent>('idle');
  if (verified) return null;

  const resend = async () => {
    setState('sending');
    try {
      await resendVerification();
      setState('sent');
    } catch (e) {
      setState(e instanceof ApiError && e.status === 429 ? 'wait' : 'error');
    }
  };

  const message = {
    idle: t('entrance.verify.idle'),
    sending: t('entrance.verify.sending'),
    sent: t('entrance.verify.sent'),
    wait: t('entrance.verify.wait'),
    error: t('entrance.verify.error'),
  }[state];

  return (
    <div role="status" className="ent-banner">
      <span>{message}</span>
      {state !== 'sent' && state !== 'sending' && (
        <OutlineButton type="button" onClick={() => void resend()}>
          {t('entrance.verify.resend')}
        </OutlineButton>
      )}
    </div>
  );
}
