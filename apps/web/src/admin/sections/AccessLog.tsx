import { Badge, Button, EmptyState, Notice, Select } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import {
  LoadMore,
  SectionHeader,
  fmtUs,
  shortHash,
  useAction,
  usePaged,
  ActionStatus,
} from '../ui';

const KINDS = [
  { value: '', label: 'Every kind' },
  { value: 'private_read', label: 'Private reads' },
  { value: 'admin_action', label: 'Admin actions' },
  { value: 'read', label: 'Reads' },
  { value: 'denied', label: 'Denied' },
];

export function AccessLogSection() {
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
        title="Access log"
        description="Every private read and admin action, newest first. Each row is chained to the one before it with a SHA-256 hash, so a changed or deleted row breaks the chain. A nightly job verifies it too."
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
            Verify chain
          </Button>
        }
      />
      <ActionStatus error={verify.error} />
      {report &&
        (report.ok ? (
          <Notice variant="success" className="ss-status" role="status" title="Chain intact">
            {report.rows_checked.toLocaleString()} rows verified. Head{' '}
            <span className="adm-mono">{shortHash(report.head_hash)}</span>
          </Notice>
        ) : (
          <Notice variant="danger" className="ss-status" role="alert" title="Chain broken">
            {report.problem ?? 'The hash chain does not verify.'}
            {report.first_bad_seq != null && <> First bad row: #{report.first_bad_seq}.</>} Treat
            this as a possible tampering incident.
          </Notice>
        ))}
      <div className="ss-toolbar">
        <Select aria-label="Filter by kind" options={KINDS} value={kind} onValueChange={setKind} />
      </div>
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState title="Nothing logged" description="No accesses match this filter." />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label="Access log">
            <thead>
              <tr>
                <th scope="col">#</th>
                <th scope="col">When</th>
                <th scope="col">Who</th>
                <th scope="col">What</th>
                <th scope="col">Reason</th>
                <th scope="col">Hash</th>
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
                        {e.target_kind ?? 'target'} <span className="adm-mono">{e.target_id}</span>
                      </div>
                    )}
                  </td>
                  <td>{e.reason}</td>
                  <td className="adm-mono" title={`prev ${e.prev_hash}`}>
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
