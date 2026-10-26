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
import { useT, type MessageKey } from '../../i18n';
import { fmtDate } from '../format';
import type { AppVersion, DevApp, VersionState } from '../types';
import { LoadError, Loading, NotAvailable } from '../ui';

const STATE: Record<
  VersionState,
  { label: MessageKey; variant: 'neutral' | 'warning' | 'success' | 'danger' }
> = {
  pending: { label: 'developers.review.statePending', variant: 'warning' },
  approved: { label: 'developers.review.stateApproved', variant: 'success' },
  rejected: { label: 'developers.review.stateRejected', variant: 'danger' },
  changes_requested: { label: 'developers.review.stateChangesRequested', variant: 'warning' },
  withdrawn: { label: 'developers.review.stateWithdrawn', variant: 'neutral' },
};

const REASON: Record<AppVersion['needs_review_reasons'][number], MessageKey> = {
  scopes: 'developers.review.reasonScopes',
  redirect_uris: 'developers.review.reasonRedirectUris',
  intents: 'developers.review.reasonIntents',
  first_release: 'developers.review.reasonFirstRelease',
};

const show = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));

function VersionCard({ v, onWithdraw }: { v: AppVersion; onWithdraw: (v: AppVersion) => void }) {
  const t = useT();
  return (
    <li className="dev-version">
      <div className="dev-version-head">
        <strong>{t('developers.review.version', { version: v.version })}</strong>
        <Badge variant={STATE[v.state].variant}>{t(STATE[v.state].label)}</Badge>
        <span className="dev-muted">
          {t('developers.review.submitted', { date: fmtDate(v.created_at) })}
        </span>
        {v.state === 'pending' && (
          <Button variant="ghost" size="sm" onClick={() => onWithdraw(v)}>
            {t('developers.review.withdrawVersion', { version: v.version })}
          </Button>
        )}
      </div>
      {v.needs_review && v.needs_review_reasons.length > 0 && (
        <p className="dev-muted">
          {t('developers.review.needsFullReview', {
            reasons: v.needs_review_reasons.map((r) => t(REASON[r])).join(', '),
          })}
        </p>
      )}
      {v.note && <p className="dev-muted">{t('developers.review.yourNote', { note: v.note })}</p>}
      {v.reviewer_note && (
        <Notice
          variant={v.state === 'approved' ? 'success' : 'warning'}
          title={t('developers.review.reviewerMessage')}
        >
          {v.reviewer_note}
        </Notice>
      )}
      {v.diff.length > 0 && (
        <details>
          <summary>
            {t('developers.review.whatChanged', {
              version: v.version,
              count: v.diff.length,
            })}
          </summary>
          <ul className="dev-diff">
            {v.diff.map((c) => (
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
  const t = useT();
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
      toast.show({ title: t('developers.review.submittedToast'), variant: 'success' });
    },
    onError: (e) => {
      setConfirm(false);
      setError(errorText(e, t('developers.review.submitFailed')));
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
      setError(errorText(e, t('developers.review.withdrawFailed')));
    },
  });

  const pending = (q.data ?? []).some((v) => v.state === 'pending');
  const suspended = app.release_state === 'suspended';

  let list;
  if (q.isPending) list = <Loading what={t('developers.review.versions')} />;
  else if (isUnavailable(q.error))
    list = <NotAvailable>{t('developers.review.notAvailable')}</NotAvailable>;
  else if (q.error)
    list = (
      <LoadError
        what={t('developers.review.versions')}
        error={q.error}
        onRetry={() => void q.refetch()}
      />
    );
  else if (q.data.length === 0) list = <p className="dev-muted">{t('developers.review.none')}</p>;
  else
    list = (
      <ul className="dev-versions" aria-label={t('developers.review.listLabel')}>
        {q.data.map((v) => (
          <VersionCard key={v.id} v={v} onWithdraw={setWithdrawing} />
        ))}
      </ul>
    );

  return (
    <div className="dev-stack">
      {suspended && (
        <Notice variant="danger" title={t('developers.review.suspendedTitle')}>
          {t('developers.review.suspendedBody')}
        </Notice>
      )}
      <section className="dev-card" aria-labelledby="dev-review-h">
        <h2 id="dev-review-h" className="dev-card-title">
          {t('developers.review.releaseHeading')}
        </h2>
        <p className="dev-muted">{t('developers.review.releaseIntro')}</p>
        <TextArea
          label={t('developers.review.noteLabel')}
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
            {t('developers.review.submitButton')}
          </Button>
        </div>
        {pending && <p className="dev-muted">{t('developers.review.alreadyWaiting')}</p>}
      </section>
      <section className="dev-card" aria-labelledby="dev-versions-h">
        <h2 id="dev-versions-h" className="dev-card-title">
          {t('developers.review.versionsHeading')}
        </h2>
        {list}
      </section>
      <ConfirmDialog
        open={confirm}
        title={t('developers.review.confirmTitle')}
        description={t('developers.review.confirmDescription')}
        confirmLabel={t('developers.review.confirmSubmit')}
        loading={submit.isPending}
        onCancel={() => setConfirm(false)}
        onConfirm={() => submit.mutate()}
      />
      <ConfirmDialog
        open={withdrawing !== null}
        danger
        title={t('developers.review.withdrawTitle', { version: withdrawing?.version ?? '' })}
        description={t('developers.review.withdrawDescription')}
        confirmLabel={t('developers.review.withdrawConfirm')}
        loading={withdraw.isPending}
        onCancel={() => setWithdrawing(null)}
        onConfirm={() => withdrawing && withdraw.mutate(withdrawing)}
      />
    </div>
  );
}
