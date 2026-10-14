// Apps tab of the Review Queues hub (ADM-CNT-001): developer applications and app version submissions
// (REVIEW_APPS). Every decision carries a note that lands in the staff access log.
import { Badge, Button, EmptyState, Notice } from '@sideroom/ui';
import type { MessageKey } from '../../i18n';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { ApiProblem, LoadMore, ReasonDialog, usePaged } from '../ui';

type Pending =
  | { kind: 'request'; row: api.DeveloperRequest; approve: boolean }
  | { kind: 'version'; row: api.AppVersion; decision: api.VersionDecision }
  | null;

const PAGE = 50;

const DECISION_TITLE: Record<api.VersionDecision, MessageKey> = {
  approve: 'admin.appsReview.decisionApprove',
  reject: 'admin.appsReview.decisionReject',
  request_changes: 'admin.appsReview.decisionChanges',
};

export function AppsReview() {
  const t = useT();
  const [pending, setPending] = useState<Pending>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const requests = usePaged<api.DeveloperRequest>('dev-requests', async (before) => {
    const items = await api.listDeveloperRequests({ state: 'pending', before });
    return { items, next: items.length >= PAGE ? items[items.length - 1]!.id : null };
  });
  const versions = usePaged<api.AppVersion>('dev-versions', async (before) => {
    const items = await api.listAppVersions({ state: 'pending', before });
    return { items, next: items.length >= PAGE ? items[items.length - 1]!.id : null };
  });

  return (
    <>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ApiProblem error={requests.error ?? versions.error} />

      <section className="ss-card" aria-labelledby="apps-req-h">
        <h2 id="apps-req-h">{t('admin.appsReview.requestsTitle')}</h2>
        {!requests.loading && requests.items.length === 0 && (
          <EmptyState title={t('admin.appsReview.noRequests')} />
        )}
        <ul className="ss-list" aria-label={t('admin.appsReview.requestsTitle')}>
          {requests.items.map((r) => (
            <li key={r.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  @{r.account.username ?? r.account_id} <Badge>{r.created_at.slice(0, 10)}</Badge>
                </div>
                <div className="ss-meta">{r.pitch}</div>
                {r.prior_work && (
                  <div className="ss-meta">
                    {t('admin.appsReview.priorWork', { work: r.prior_work })}
                  </div>
                )}
              </div>
              <Button
                size="sm"
                aria-label={t('admin.appsReview.approveRequestAria', {
                  name: r.account.username ?? r.account_id,
                })}
                onClick={() => setPending({ kind: 'request', row: r, approve: true })}
              >
                {t('admin.appsReview.approve')}
              </Button>
              <Button
                size="sm"
                variant="danger"
                aria-label={t('admin.appsReview.rejectRequestAria', {
                  name: r.account.username ?? r.account_id,
                })}
                onClick={() => setPending({ kind: 'request', row: r, approve: false })}
              >
                {t('admin.appsReview.reject')}
              </Button>
            </li>
          ))}
        </ul>
        <LoadMore paged={requests} />
      </section>

      <section className="ss-card" aria-labelledby="apps-ver-h">
        <h2 id="apps-ver-h">{t('admin.appsReview.versionsTitle')}</h2>
        {!versions.loading && versions.items.length === 0 && (
          <EmptyState title={t('admin.appsReview.noVersions')} />
        )}
        <ul className="ss-list" aria-label={t('admin.appsReview.versionsTitle')}>
          {versions.items.map((v) => (
            <li key={v.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t('admin.appsReview.versionName', {
                    name: v.application.name,
                    version: v.version,
                  })}{' '}
                  {v.needs_review && (
                    <Badge variant="warning">{t('admin.appsReview.needsReview')}</Badge>
                  )}
                </div>
                {v.needs_review_reasons.length > 0 && (
                  <div className="ss-meta">{v.needs_review_reasons.join(', ')}</div>
                )}
                {v.diff.length > 0 && (
                  <ul className="ss-meta" aria-label={t('admin.appsReview.diffLabel')}>
                    {v.diff.map((d) => (
                      <li key={d.key}>
                        <span className="adm-mono">{d.key}</span>: {JSON.stringify(d.old ?? null)} →{' '}
                        {JSON.stringify(d.new ?? null)}
                      </li>
                    ))}
                  </ul>
                )}
                {v.note && <div className="ss-meta">{v.note}</div>}
              </div>
              <Button
                size="sm"
                aria-label={t('admin.appsReview.approveVersionAria', {
                  name: v.application.name,
                  version: v.version,
                })}
                onClick={() => setPending({ kind: 'version', row: v, decision: 'approve' })}
              >
                {t('admin.appsReview.approve')}
              </Button>
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.appsReview.changesVersionAria', {
                  name: v.application.name,
                  version: v.version,
                })}
                onClick={() => setPending({ kind: 'version', row: v, decision: 'request_changes' })}
              >
                {t('admin.appsReview.requestChanges')}
              </Button>
              <Button
                size="sm"
                variant="danger"
                aria-label={t('admin.appsReview.rejectVersionAria', {
                  name: v.application.name,
                  version: v.version,
                })}
                onClick={() => setPending({ kind: 'version', row: v, decision: 'reject' })}
              >
                {t('admin.appsReview.reject')}
              </Button>
            </li>
          ))}
        </ul>
        <LoadMore paged={versions} />
      </section>

      <ReasonDialog
        open={pending !== null}
        title={
          pending?.kind === 'request'
            ? t(
                pending.approve
                  ? 'admin.appsReview.approveRequest'
                  : 'admin.appsReview.rejectRequest',
              )
            : pending?.kind === 'version'
              ? t(DECISION_TITLE[pending.decision])
              : ''
        }
        confirmLabel={t('admin.appsReview.confirm')}
        danger={
          (pending?.kind === 'request' && !pending.approve) ||
          (pending?.kind === 'version' && pending.decision === 'reject')
        }
        reasonLabel={t('admin.appsReview.noteLabel')}
        reasonOptional={
          (pending?.kind === 'request' && pending.approve) ||
          (pending?.kind === 'version' && pending.decision === 'approve')
        }
        onCancel={() => setPending(null)}
        onConfirm={async (note) => {
          if (!pending) return;
          if (pending.kind === 'request') {
            await api.decideDeveloperRequest(pending.row.id, pending.approve, note);
            requests.reload();
          } else {
            await api.decideAppVersion(pending.row.id, pending.decision, note);
            versions.reload();
          }
          setPending(null);
          setNotice(t('admin.appsReview.decided'));
        }}
      />
    </>
  );
}
