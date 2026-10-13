import { Link, useSearch } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { cancelEmailChange } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { minutesFromSeconds } from '../../lib/validation';

type State = 'working' | 'missing' | 'done' | 'invalid' | 'throttled' | 'error';

/**
 * The "This wasn't me" link from the email-change notice (D-167): `/email-change/cancel?token=…`. The token
 * is single use, so it is posted once. While the change is still pending it is cancelled; once it has
 * completed, every session and trusted device is signed out instead (the server decides which).
 */
export function EmailChangeCancel() {
  const { token } = useSearch({ strict: false }) as { token?: string };
  const [state, setState] = useState<State>(token ? 'working' : 'missing');
  const [retryAfter, setRetryAfter] = useState<number | null>(null);
  const sent = useRef(false);

  useEffect(() => {
    if (!token || sent.current) return;
    sent.current = true;
    cancelEmailChange(token)
      .then(() => setState('done'))
      .catch((e: unknown) => {
        if (e instanceof ApiError && e.status === 429) {
          setRetryAfter(e.retryAfter);
          setState('throttled');
        } else setState(e instanceof ApiError && e.status < 500 ? 'invalid' : 'error');
      });
  }, [token]);

  const minutes = minutesFromSeconds(retryAfter);
  return (
    <AuthSplit
      eyebrow="Wasn't you?"
      heading="Consider it"
      accent="stopped."
      copy="If someone else asked to change your email, this undoes it."
      footLabel="Member entrance / 05"
    >
      <FormHead eyebrow="Email change" title="Securing your account." />
      {state === 'working' && (
        <p role="status" className="ent-sub">
          One moment…
        </p>
      )}
      {state === 'done' && (
        <>
          <p role="status" className="ent-sub">
            Done. If the change had not gone through yet, it is cancelled and your email is
            unchanged. If it had, every device is now signed out.
          </p>
          <p className="ent-sub">
            Someone may know your password. Use <Link to="/forgot">Forgot password</Link> to set a
            new one, and turn on two-factor authentication in Settings.
          </p>
        </>
      )}
      {state === 'missing' && (
        <p role="alert" className="ent-sub">
          This link is missing its token. Open the full link from the email again.
        </p>
      )}
      {state === 'invalid' && (
        <p role="alert" className="ent-sub">
          This link is invalid, has expired, or was already used. Links work once and last seven
          days. If you still think someone has your account, sign in and change your password, or
          use Forgot password.
        </p>
      )}
      {state === 'throttled' && (
        <p role="alert" className="ent-sub">
          {`Too many tries. Try again in ${minutes} minute${minutes === 1 ? '' : 's'}.`}
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
