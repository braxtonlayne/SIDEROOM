import { Link, useParams } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { verifyEmail } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';

type State = 'verifying' | 'done' | 'expired' | 'error';

/** ENT-UI-020: `/verify-email/:token`. The token is single use, so the request is sent once. */
export function VerifyEmail() {
  const { token } = useParams({ strict: false }) as { token: string };
  const [state, setState] = useState<State>('verifying');
  const sent = useRef(false);

  useEffect(() => {
    if (sent.current) return;
    sent.current = true;
    verifyEmail(token)
      .then(() => setState('done'))
      .catch((e: unknown) => {
        setState(e instanceof ApiError && e.status === 400 ? 'expired' : 'error');
      });
  }, [token]);

  return (
    <AuthSplit
      eyebrow="Almost there."
      heading="Check the"
      accent="door."
      copy="One click confirms this address belongs to you."
      footLabel="Member entrance / 03"
    >
      <FormHead eyebrow="Verify email" title="Confirming your address." />
      {state === 'verifying' && (
        <p role="status" className="ent-sub">
          One moment…
        </p>
      )}
      {state === 'done' && (
        <p role="status" className="ent-sub">
          Your email is verified. Thanks for confirming.
        </p>
      )}
      {state === 'expired' && (
        <p role="alert" className="ent-sub">
          That link has expired or was already used. Sign in and ask for a new one from the banner
          in the lounge.
        </p>
      )}
      {state === 'error' && (
        <p role="alert" className="ent-sub">
          We couldn't reach the door. Check your connection and try again.
        </p>
      )}
      <p className="ent-footnote">
        <Link to="/login">Back to sign in</Link>
      </p>
    </AuthSplit>
  );
}
