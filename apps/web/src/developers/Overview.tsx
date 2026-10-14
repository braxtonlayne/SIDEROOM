import { Badge, Button, Checkbox, Notice, TextArea, useToast } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { applyAsDeveloper, errorText, getDeveloperStatus, isUnavailable, keys } from './api';
import { useT } from '../i18n';
import { fmtDate } from './format';
import { Loading } from './ui';

function ApplyForm() {
  const t = useT();
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
      toast.show({ title: t('developers.overview.applicationSent'), variant: 'success' });
    },
    onError: (e) => setError(errorText(e, t('developers.overview.sendFailed'))),
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
        {t('developers.overview.applyHeading')}
      </h2>
      <p className="dev-muted">{t('developers.overview.applyIntro')}</p>
      <TextArea
        label={t('developers.overview.pitchLabel')}
        value={pitch}
        maxLength={2000}
        onChange={(e) => setPitch(e.target.value)}
        required
      />
      <TextArea
        label={t('developers.overview.priorLabel')}
        value={prior}
        maxLength={2000}
        onChange={(e) => setPrior(e.target.value)}
      />
      <Checkbox
        label={t('developers.overview.agree')}
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
          {t('developers.overview.sendButton')}
        </Button>
      </div>
    </form>
  );
}

function Status() {
  const t = useT();
  const q = useQuery({ queryKey: keys.status, queryFn: getDeveloperStatus });
  if (q.isPending) return <Loading what={t('developers.overview.yourDeveloperStatus')} />;
  if (q.error instanceof ApiError && q.error.status === 401)
    return (
      <Notice variant="info" title={t('developers.overview.signInTitle')}>
        <Link to="/login" search={{ next: '/developers' }} className="dev-link">
          {t('developers.overview.logIn')}
        </Link>{' '}
        {t('developers.overview.signInRest')}
      </Notice>
    );
  if (isUnavailable(q.error))
    return (
      <Notice variant="info" title={t('developers.overview.notAvailableTitle')}>
        {t('developers.overview.notAvailableBody')}
      </Notice>
    );
  if (q.error)
    return (
      <Notice variant="danger" title={t('developers.overview.loadErrorTitle')}>
        {errorText(q.error)}{' '}
        <Button variant="link" size="sm" onClick={() => void q.refetch()}>
          {t('developers.overview.tryAgain')}
        </Button>
      </Notice>
    );
  const { is_developer: dev, request: r } = q.data;
  if (dev)
    return (
      <Notice variant="success" title={t('developers.overview.approvedTitle')}>
        {t('developers.overview.approvedThanks')}{' '}
        <Link to="/developers/applications" className="dev-link">
          {t('developers.overview.goToApplications')}
        </Link>
        .
      </Notice>
    );
  if (r?.state === 'pending')
    return (
      <section className="dev-card" aria-labelledby="dev-status-h">
        <h2 id="dev-status-h" className="dev-card-title">
          {t('developers.overview.pendingHeading')}{' '}
          <Badge variant="warning">{t('developers.overview.waitingForReview')}</Badge>
        </h2>
        <p className="dev-muted">
          {t('developers.overview.pendingSent', { date: fmtDate(r.created_at) })}
        </p>
      </section>
    );
  return (
    <>
      {r?.state === 'rejected' && (
        <Notice variant="warning" title={t('developers.overview.rejectedTitle')}>
          {t('developers.overview.rejectedBody', {
            note: r.reviewer_note ?? t('developers.overview.noReason'),
          })}
        </Notice>
      )}
      {r?.state === 'suspended' && (
        <Notice variant="danger" title={t('developers.overview.suspendedTitle')}>
          {r.reviewer_note ?? t('developers.overview.contactStaff')}
        </Notice>
      )}
      {r?.state !== 'suspended' && <ApplyForm />}
    </>
  );
}

/** /developers landing: what the platform offers, where to start, and the developer application. */
export function OverviewPage() {
  const t = useT();
  return (
    <>
      <h1 className="dev-title">{t('developers.overview.heading')}</h1>
      <p className="dev-lede">{t('developers.overview.lede')}</p>
      <ul className="dev-doc-grid" aria-label={t('developers.overview.startLabel')}>
        <li>
          <Link to="/developers/applications" className="dev-app-card">
            <span className="dev-app-card-text">
              <strong>{t('developers.overview.cardApplicationsTitle')}</strong>
              <span className="dev-muted">{t('developers.overview.cardApplicationsBody')}</span>
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
              <strong>{t('developers.overview.cardStartTitle')}</strong>
              <span className="dev-muted">{t('developers.overview.cardStartBody')}</span>
            </span>
          </Link>
        </li>
        <li>
          <Link to="/developers/docs" className="dev-app-card">
            <span className="dev-app-card-text">
              <strong>{t('developers.overview.cardDocsTitle')}</strong>
              <span className="dev-muted">{t('developers.overview.cardDocsBody')}</span>
            </span>
          </Link>
        </li>
      </ul>
      <Status />
    </>
  );
}
