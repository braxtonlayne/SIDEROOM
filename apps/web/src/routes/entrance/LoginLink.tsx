import { Link, useRouter, useSearch } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { loginWithLink } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { minutesFromSeconds } from '../../lib/validation';

type State =
  | { kind: 'signing-in' }
  | { kind: 'missing' }
  | { kind: 'expired' }
  | { kind: 'throttled'; retryAfter: number | null }
  | { kind: 'error' };

const NETWORK = "We couldn't reach the door. Check your connection and try again.";

/**
 * Owner one-time login link: `/login/link?token=…`. Posts the token once, shows progress, and on success
 * continues to the Lounge. A failed link explains how to get a new one; it never leaves a session behind.
 */
export function LoginLink() {
  const router = useRouter();
  const { token } = useSearch({ strict: false }) as { token?: string };
  const [state, setState] = useState<State>(() =>
    token ? { kind: 'signing-in' } : { kind: 'missing' },
  );
  const sent = useRef(false);

  useEffect(() => {
    if (!token || sent.current) return;
    sent.current = true;
    loginWithLink(token)
      .then(() => router.navigate({ to: '/channels', replace: true }))
      .catch((e: unknown) => {
        if (e instanceof ApiError && e.status === 429)
          setState({ kind: 'throttled', retryAfter: e.retryAfter });
        else if (e instanceof ApiError && e.status < 500) setState({ kind: 'expired' });
        else setState({ kind: 'error' });
      });
  }, [token, router]);

  return (
    <AuthSplit
      eyebrow="Owner access."
      heading="One door,"
      accent="one use."
      copy="This link signs you in once. It does not reset your password."
      footLabel="Member entrance / 04"
    >
      <FormHead eyebrow="Login link" title="Signing you in." />
      {state.kind === 'signing-in' && (
        <p role="status" className="ent-sub">
          One moment…
        </p>
      )}
      {state.kind === 'missing' && (
        <p role="alert" className="ent-sub">
          This link is missing its token. Open the full link from the terminal output again.
        </p>
      )}
      {state.kind === 'expired' && (
        <p role="alert" className="ent-sub">
          This link is invalid, has expired, or was already used. Links work once and last an hour.
          Run the bootstrap command again for a new one.
        </p>
      )}
      {state.kind === 'throttled' && (
        <p role="alert" className="ent-sub">
          {`Too many tries. Try again in ${minutesFromSeconds(state.retryAfter)} minute${
            minutesFromSeconds(state.retryAfter) === 1 ? '' : 's'
          }.`}
        </p>
      )}
      {state.kind === 'error' && (
        <p role="alert" className="ent-sub">
          {NETWORK}
        </p>
      )}
      <p className="ent-footnote">
        <Link to="/login">Use your password instead</Link>
      </p>
    </AuthSplit>
  );
}
