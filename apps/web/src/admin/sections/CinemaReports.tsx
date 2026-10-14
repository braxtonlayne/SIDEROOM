// Cinema tab of the Review Queues hub (ADM-CNT-001): reported reviews and comments (DELETE_CONTENT).
import { Badge, Button, EmptyState, Select } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { ApiProblem, LoadMore, ReasonDialog, StateBadge, fmtMs, usePaged } from '../ui';

type Pending = { row: api.CinemaReport; action: 'hide' | 'dismiss' } | null;

export function CinemaReports() {
  const t = useT();
  const [status, setStatus] = useState('open');
  const [pending, setPending] = useState<Pending>(null);
  const paged = usePaged<api.CinemaReport>(`cin-reports:${status}`, async (before) => {
    const p = await api.listCinemaReports({ status, before });
    return { items: p.items, next: p.next_before };
  });

  return (
    <>
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.cinemaReports.filterStatus')}
          options={[
            { value: 'open', label: t('admin.cinemaReports.open') },
            { value: 'actioned', label: t('admin.cinemaReports.actioned') },
            { value: 'dismissed', label: t('admin.cinemaReports.dismissed') },
            { value: 'all', label: t('admin.cinemaReports.all') },
          ]}
          value={status}
          onValueChange={setStatus}
        />
      </div>
      <ApiProblem error={paged.error} />
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.cinemaReports.emptyTitle')}
          description={t('admin.cinemaReports.emptyDescription')}
        />
      )}
      <ul className="ss-list" aria-label={t('admin.cinemaReports.listLabel')}>
        {paged.items.map((r) => (
          <li key={r.id} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">
                {r.target_kind === 'review'
                  ? t('admin.cinemaReports.kindReview')
                  : t('admin.cinemaReports.kindComment')}{' '}
                <StateBadge state={r.status} />{' '}
                {r.content?.hidden && <Badge>{t('admin.cinemaReports.hidden')}</Badge>}
              </div>
              <div className="ss-meta">
                {r.content ? r.content.excerpt : t('admin.cinemaReports.gone')}
              </div>
              <div className="ss-meta">
                {t('admin.cinemaReports.reportedFor', {
                  reason: r.reason || t('admin.cinemaReports.noReason'),
                  when: fmtMs(r.created_at_ms),
                })}
              </div>
            </div>
            {r.status === 'open' && (
              <>
                {r.content && !r.content.hidden && (
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('admin.cinemaReports.hideAria', { id: r.id })}
                    onClick={() => setPending({ row: r, action: 'hide' })}
                  >
                    {t('admin.cinemaReports.hide')}
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.cinemaReports.dismissAria', { id: r.id })}
                  onClick={() => setPending({ row: r, action: 'dismiss' })}
                >
                  {t('admin.cinemaReports.dismiss')}
                </Button>
              </>
            )}
          </li>
        ))}
      </ul>
      <LoadMore paged={paged} />
      <ReasonDialog
        open={!!pending}
        title={
          pending?.action === 'hide'
            ? t('admin.cinemaReports.hideTitle')
            : t('admin.cinemaReports.dismissTitle')
        }
        description={
          pending?.action === 'hide' ? t('admin.cinemaReports.hideDescription') : undefined
        }
        confirmLabel={
          pending?.action === 'hide'
            ? t('admin.cinemaReports.hide')
            : t('admin.cinemaReports.dismiss')
        }
        danger={pending?.action === 'hide'}
        reasonOptional
        onCancel={() => setPending(null)}
        onConfirm={async (reason) => {
          if (!pending) return;
          if (pending.action === 'hide')
            await api.hideCinemaContent(
              pending.row.target_kind,
              pending.row.target_id,
              true,
              reason,
            );
          else await api.resolveCinemaReport(pending.row.id, 'dismissed', reason);
          setPending(null);
          paged.reload();
        }}
      />
    </>
  );
}
