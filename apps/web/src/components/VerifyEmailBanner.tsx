import { OutlineButton } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { resendVerification } from '../api/entrance';

type Sent = 'idle' | 'sending' | 'sent' | 'wait' | 'error';

/**
 * ENT-UI-020: "verify your email" banner for signed-in members whose address is not confirmed yet.
 * The caller decides `verified` (from the account); the banner renders nothing once it is true.
 */
export function VerifyEmailBanner({ verified }: { verified: boolean }) {
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
    idle: 'Verify your email to keep your account safe.',
    sending: 'Sending a fresh link…',
    sent: 'Check your inbox for the link.',
    wait: 'A link was sent a moment ago. Try again in a minute.',
    error: "We couldn't send the link. Try again in a moment.",
  }[state];

  return (
    <div role="status" className="ent-banner">
      <span>{message}</span>
      {state !== 'sent' && state !== 'sending' && (
        <OutlineButton type="button" onClick={() => void resend()}>
          Resend link
        </OutlineButton>
      )}
    </div>
  );
}
