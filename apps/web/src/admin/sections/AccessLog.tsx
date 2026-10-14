import { Badge, Button, EmptyState, Notice, Select } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import {
  LoadMore,
  SectionHeader,
  fmtUs,
  shortHash,
  useAction,
  usePaged,
  ActionStatus,
} from '../ui';

const KINDS: { value: string; label: MessageKey }[] = [
  { value: '', label: 'admin.accessLog.kindAll' },
  { value: 'private_read', label: 'admin.accessLog.kindPrivateRead' },
  { value: 'admin_action', label: 'admin.accessLog.kindAdminAction' },
  { value: 'read', label: 'admin.accessLog.kindRead' },
  { value: 'denied', label: 'admin.accessLog.kindDenied' },
];

export function AccessLogSection() {
  const t = useT();
  const kindOptions = KINDS.map((k) => ({ value: k.value, label: t(k.label) }));
  const [kind, setKind] = useState('');
  const [report, setReport] = useState<api.ChainReport | null>(null);
  const verify = useAction();
  const paged = usePaged<api.AccessLogEntry, number>(`access:${kind}`, async (before) => {
    const p = await api.listAccessLog({ kind, before, limit: 50 });
    return { items: p.items, next: p.next_before };
  });

  return (
    <>
      <SectionHeader
        title={t('admin.accessLog.title')}
        description={t('admin.accessLog.description')}
        actions={
          <Button
            variant="secondary"
            loading={verify.busy}
            onClick={() =>
              void verify.run(async () => {
                setReport(await api.verifyAccessLog());
              })
            }
          >
            {t('admin.accessLog.verifyChain')}
          </Button>
        }
      />
      <ActionStatus error={verify.error} />
      {report &&
        (report.ok ? (
          <Notice
            variant="success"
            className="ss-status"
            role="status"
            title={t('admin.accessLog.chainIntact')}
          >
            {t('admin.accessLog.rowsVerified', { count: report.rows_checked.toLocaleString() })}{' '}
            <span className="adm-mono">{shortHash(report.head_hash)}</span>
          </Notice>
        ) : (
          <Notice
            variant="danger"
            className="ss-status"
            role="alert"
            title={t('admin.accessLog.chainBroken')}
          >
            {report.problem ?? t('admin.accessLog.chainNotVerified')}
            {report.first_bad_seq != null && (
              <> {t('admin.accessLog.firstBadRow', { seq: report.first_bad_seq })}</>
            )}{' '}
            {t('admin.accessLog.tamperingWarning')}
          </Notice>
        ))}
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.accessLog.filterByKind')}
          options={kindOptions}
          value={kind}
          onValueChange={setKind}
        />
      </div>
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.accessLog.emptyTitle')}
          description={t('admin.accessLog.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={t('admin.accessLog.tableLabel')}>
            <thead>
              <tr>
                <th scope="col">#</th>
                <th scope="col">{t('admin.accessLog.colWhen')}</th>
                <th scope="col">{t('admin.accessLog.colWho')}</th>
                <th scope="col">{t('admin.accessLog.colWhat')}</th>
                <th scope="col">{t('admin.accessLog.colReason')}</th>
                <th scope="col">{t('admin.accessLog.colHash')}</th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((e) => (
                <tr key={e.seq}>
                  <td>{e.seq}</td>
                  <td>{fmtUs(e.at_us)}</td>
                  <td className="adm-mono">{e.actor_id}</td>
                  <td>
                    <Badge
                      variant={
                        e.kind === 'denied'
                          ? 'danger'
                          : e.kind === 'private_read'
                            ? 'warning'
                            : 'neutral'
                      }
                    >
                      {e.kind.replace('_', ' ')}
                    </Badge>{' '}
                    {e.action}
                    {e.target_id && (
                      <div className="ss-meta">
                        {e.target_kind ?? t('admin.accessLog.target')}{' '}
                        <span className="adm-mono">{e.target_id}</span>
                      </div>
                    )}
                  </td>
                  <td>{e.reason}</td>
                  <td
                    className="adm-mono"
                    title={t('admin.accessLog.prevHash', { hash: e.prev_hash })}
                  >
                    {shortHash(e.hash)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
    </>
  );
}
