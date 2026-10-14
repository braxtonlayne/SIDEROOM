import { Badge, Button, Checkbox, Notice, TextArea, useToast } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { applyAsDeveloper, errorText, getDeveloperStatus, isUnavailable, keys } from './api';
import { fmtDate } from './format';
import { Loading } from './ui';

function ApplyForm() {
  const qc = useQueryClient();
  const toast = useToast();
  const [pitch, setPitch] = useState('');
  const [prior, setPrior] = useState('');
  const [agree, setAgree] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const send = useMutation({
    mutationFn: () =>
      applyAsDeveloper({
        pitch: pitch.trim(),
        ...(prior.trim() ? { prior_work: prior.trim() } : {}),
        accepted_terms: true,
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.status });
      toast.show({ title: 'Application sent.', variant: 'success' });
    },
    onError: (e) => setError(errorText(e, 'Could not send your application.')),
  });
  const ok = pitch.trim().length > 0 && pitch.length <= 2000 && agree;
  return (
    <form
      className="dev-form dev-card"
      aria-labelledby="dev-apply-h"
      onSubmit={(e) => {
        e.preventDefault();
        if (!ok) return;
        setError(null);
        send.mutate();
      }}
    >
      <h2 id="dev-apply-h" className="dev-card-title">
        Apply to build apps
      </h2>
      <p className="dev-muted">
        You can already make apps and try them in small test servers. Approval is a mark of trust
        that staff give after reading this.
      </p>
      <TextArea
        label="What do you want to build?"
        value={pitch}
        maxLength={2000}
        onChange={(e) => setPitch(e.target.value)}
        required
      />
      <TextArea
        label="Anything you have built before? (optional)"
        value={prior}
        maxLength={2000}
        onChange={(e) => setPrior(e.target.value)}
      />
      <Checkbox
        label="I agree to the developer terms."
        checked={agree}
        onChange={(e) => setAgree(e.target.checked)}
      />
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      <div className="dev-actions">
        <Button type="submit" disabled={!ok} loading={send.isPending}>
          Send application
        </Button>
      </div>
    </form>
  );
}

function Status() {
  const q = useQuery({ queryKey: keys.status, queryFn: getDeveloperStatus });
  if (q.isPending) return <Loading what="your developer status" />;
  if (q.error instanceof ApiError && q.error.status === 401)
    return (
      <Notice variant="info" title="Sign in to apply">
        <Link to="/login" search={{ next: '/developers' }} className="dev-link">
          Log in
        </Link>{' '}
        to apply and to manage your applications.
      </Notice>
    );
  if (isUnavailable(q.error))
    return (
      <Notice variant="info" title="Not available yet">
        This server does not take developer applications yet.
      </Notice>
    );
  if (q.error)
    return (
      <Notice variant="danger" title="Couldn't load your developer status">
        {errorText(q.error)}{' '}
        <Button variant="link" size="sm" onClick={() => void q.refetch()}>
          Try again
        </Button>
      </Notice>
    );
  const { is_developer: dev, request: r } = q.data;
  if (dev)
    return (
      <Notice variant="success" title="You are an approved developer">
        Thank you for building here.{' '}
        <Link to="/developers/applications" className="dev-link">
          Go to your applications
        </Link>
        .
      </Notice>
    );
  if (r?.state === 'pending')
    return (
      <section className="dev-card" aria-labelledby="dev-status-h">
        <h2 id="dev-status-h" className="dev-card-title">
          Your application <Badge variant="warning">Waiting for review</Badge>
        </h2>
        <p className="dev-muted">Sent {fmtDate(r.created_at)}. Staff will look at it soon.</p>
      </section>
    );
  return (
    <>
      {r?.state === 'rejected' && (
        <Notice variant="warning" title="Your last application was not approved">
          {r.reviewer_note ?? 'No reason was given.'} You can apply again below.
        </Notice>
      )}
      {r?.state === 'suspended' && (
        <Notice variant="danger" title="Developer access suspended">
          {r.reviewer_note ?? 'Contact staff for details.'}
        </Notice>
      )}
      {r?.state !== 'suspended' && <ApplyForm />}
    </>
  );
}

/** /developers landing: what the platform offers, where to start, and the developer application. */
export function OverviewPage() {
  return (
    <>
      <h1 className="dev-title">Build on SIDE ROOM</h1>
      <p className="dev-lede">
        Bots, slash commands and sign-in with SIDE ROOM, over an API shaped like Discord&apos;s so
        existing libraries and bots port with a base URL change.
      </p>
      <ul className="dev-doc-grid" aria-label="Where to start">
        <li>
          <Link to="/developers/applications" className="dev-app-card">
            <span className="dev-app-card-text">
              <strong>Your applications</strong>
              <span className="dev-muted">
                Create an app, get OAuth2 credentials, a bot and a way to release it.
              </span>
            </span>
          </Link>
        </li>
        <li>
          <Link
            to="/developers/docs/$page"
            params={{ page: 'getting-started' }}
            className="dev-app-card"
          >
            <span className="dev-app-card-text">
              <strong>Getting started</strong>
              <span className="dev-muted">Make a bot and send your first request in minutes.</span>
            </span>
          </Link>
        </li>
        <li>
          <Link to="/developers/docs" className="dev-app-card">
            <span className="dev-app-card-text">
              <strong>Documentation</strong>
              <span className="dev-muted">
                Authentication, gateway, REST, rate limits and interactions.
              </span>
            </span>
          </Link>
        </li>
      </ul>
      <Status />
    </>
  );
}
