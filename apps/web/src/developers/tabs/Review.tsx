import { Badge, Button, ConfirmDialog, Notice, TextArea, useToast } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import {
  errorText,
  isUnavailable,
  keys,
  listVersions,
  submitVersion,
  withdrawVersion,
} from '../api';
import { fmtDate } from '../format';
import type { AppVersion, DevApp, VersionState } from '../types';
import { LoadError, Loading, NotAvailable } from '../ui';

const STATE: Record<
  VersionState,
  { label: string; variant: 'neutral' | 'warning' | 'success' | 'danger' }
> = {
  pending: { label: 'In review', variant: 'warning' },
  approved: { label: 'Approved', variant: 'success' },
  rejected: { label: 'Rejected', variant: 'danger' },
  changes_requested: { label: 'Changes requested', variant: 'warning' },
  withdrawn: { label: 'Withdrawn', variant: 'neutral' },
};

const REASON: Record<AppVersion['needs_review_reasons'][number], string> = {
  scopes: 'scopes changed',
  redirect_uris: 'redirect URIs changed',
  intents: 'privileged intents changed',
  first_release: 'first release',
};

const show = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));

function VersionCard({ v, onWithdraw }: { v: AppVersion; onWithdraw: (v: AppVersion) => void }) {
  return (
    <li className="dev-version">
      <div className="dev-version-head">
        <strong>Version {v.version}</strong>
        <Badge variant={STATE[v.state].variant}>{STATE[v.state].label}</Badge>
        <span className="dev-muted">Submitted {fmtDate(v.created_at)}</span>
        {v.state === 'pending' && (
          <Button variant="ghost" size="sm" onClick={() => onWithdraw(v)}>
            {`Withdraw version ${v.version}`}
          </Button>
        )}
      </div>
      {v.needs_review && v.needs_review_reasons.length > 0 && (
        <p className="dev-muted">
          Needs a full review: {v.needs_review_reasons.map((r) => REASON[r]).join(', ')}.
        </p>
      )}
      {v.note && <p className="dev-muted">Your note: {v.note}</p>}
      {v.reviewer_note && (
        <Notice
          variant={v.state === 'approved' ? 'success' : 'warning'}
          title="Message from the reviewer"
        >
          {v.reviewer_note}
        </Notice>
      )}
      {v.diff.changed.length > 0 && (
        <details>
          <summary>{`What changed in version ${v.version} (${v.diff.changed.length})`}</summary>
          <ul className="dev-diff">
            {v.diff.changed.map((c) => (
              <li key={c.key}>
                <code>{c.key}</code>: <del>{show(c.old)}</del> <ins>{show(c.new)}</ins>
              </li>
            ))}
          </ul>
        </details>
      )}
    </li>
  );
}

export function ReviewTab({ app }: { app: DevApp }) {
  const qc = useQueryClient();
  const toast = useToast();
  const q = useQuery({ queryKey: keys.versions(app.id), queryFn: () => listVersions(app.id) });
  const [note, setNote] = useState('');
  const [confirm, setConfirm] = useState(false);
  const [withdrawing, setWithdrawing] = useState<AppVersion | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: keys.versions(app.id) });
    void qc.invalidateQueries({ queryKey: keys.app(app.id) });
    void qc.invalidateQueries({ queryKey: keys.apps });
  };
  const submit = useMutation({
    mutationFn: () => submitVersion(app.id, note.trim() || undefined),
    onSuccess: () => {
      setConfirm(false);
      setNote('');
      refresh();
      toast.show({ title: 'Submitted for review.', variant: 'success' });
    },
    onError: (e) => {
      setConfirm(false);
      setError(errorText(e, 'Could not submit the version.'));
    },
  });
  const withdraw = useMutation({
    mutationFn: (v: AppVersion) => withdrawVersion(app.id, v.id),
    onSuccess: () => {
      setWithdrawing(null);
      refresh();
    },
    onError: (e) => {
      setWithdrawing(null);
      setError(errorText(e, 'Could not withdraw the version.'));
    },
  });

  const pending = (q.data ?? []).some((v) => v.state === 'pending');
  const suspended = app.release_state === 'suspended';

  let list;
  if (q.isPending) list = <Loading what="versions" />;
  else if (isUnavailable(q.error))
    list = <NotAvailable>The server has no app review yet.</NotAvailable>;
  else if (q.error)
    list = <LoadError what="versions" error={q.error} onRetry={() => void q.refetch()} />;
  else if (q.data.length === 0) list = <p className="dev-muted">No versions submitted yet.</p>;
  else
    list = (
      <ul className="dev-versions" aria-label="Versions">
        {q.data.map((v) => (
          <VersionCard key={v.id} v={v} onWithdraw={setWithdrawing} />
        ))}
      </ul>
    );

  return (
    <div className="dev-stack">
      {suspended && (
        <Notice variant="danger" title="This application is suspended">
          Its bot and OAuth tokens do not work. Contact staff to find out why.
        </Notice>
      )}
      <section className="dev-card" aria-labelledby="dev-review-h">
        <h2 id="dev-review-h" className="dev-card-title">
          Release review
        </h2>
        <p className="dev-muted">
          Submitting snapshots your current settings (name, description, scopes, redirects,
          privileged intents and links) for staff to review. Approved apps are released and can be
          installed in any server. Changing scopes, redirects or intents later needs another review.
        </p>
        <TextArea
          label="Note for the reviewer (optional)"
          value={note}
          maxLength={500}
          onChange={(e) => setNote(e.target.value)}
        />
        {error && (
          <p className="dev-error" role="alert">
            {error}
          </p>
        )}
        <div className="dev-actions">
          <Button disabled={pending || suspended} onClick={() => setConfirm(true)}>
            Submit for review
          </Button>
        </div>
        {pending && <p className="dev-muted">A version is already waiting for review.</p>}
      </section>
      <section className="dev-card" aria-labelledby="dev-versions-h">
        <h2 id="dev-versions-h" className="dev-card-title">
          Versions
        </h2>
        {list}
      </section>
      <ConfirmDialog
        open={confirm}
        title="Submit for review?"
        description="Your current settings are sent to staff. You can withdraw the version while it is waiting."
        confirmLabel="Submit"
        loading={submit.isPending}
        onCancel={() => setConfirm(false)}
        onConfirm={() => submit.mutate()}
      />
      <ConfirmDialog
        open={withdrawing !== null}
        danger
        title={`Withdraw version ${withdrawing?.version ?? ''}?`}
        description="It leaves the review queue. You can submit again later."
        confirmLabel="Withdraw"
        loading={withdraw.isPending}
        onCancel={() => setWithdrawing(null)}
        onConfirm={() => withdrawing && withdraw.mutate(withdrawing)}
      />
    </div>
  );
}
