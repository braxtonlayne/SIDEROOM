import { Badge, Button, EmptyState, Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { LoadMore, ReasonDialog, SectionHeader, fmtMs, usePaged } from '../ui';

export function BansSection() {
  const [activeOnly, setActiveOnly] = useState(true);
  const [target, setTarget] = useState<api.BanRecord | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const paged = usePaged<api.BanRecord>(`bans:${activeOnly}`, async (after) => {
    const p = await api.listBans({ active_only: activeOnly, after, limit: 50 });
    return { items: p.items, next: p.next_after };
  });

  return (
    <>
      <SectionHeader
        title="Platform bans"
        description="A ban applies to a Person, so it covers every account they hold. Lifting a ban reactivates accounts while the account cap allows."
      />
      <div className="ss-toolbar">
        <Switch
          label="Only active bans"
          checked={activeOnly}
          onChange={(e) => setActiveOnly(e.target.checked)}
        />
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState title="No bans" description="Nobody is platform-banned." />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label="Platform bans">
          {paged.items.map((b) => (
            <li key={b.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  Person <span className="adm-mono">{b.person_id}</span>{' '}
                  <Badge variant={b.category === 'serious' ? 'danger' : 'warning'}>
                    {b.category}
                  </Badge>{' '}
                  {b.lifted_at_ms ? <Badge>lifted</Badge> : <Badge variant="danger">active</Badge>}
                </div>
                <div className="ss-meta">
                  {b.note || 'No note'} · banned {fmtMs(b.created_at_ms)}
                  {b.lifted_at_ms ? ` · lifted ${fmtMs(b.lifted_at_ms)}` : ''}
                </div>
              </div>
              {!b.lifted_at_ms && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`Lift ban on person ${b.person_id}`}
                  onClick={() => setTarget(b)}
                >
                  Lift ban
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />
      <ReasonDialog
        open={!!target}
        title="Lift this ban"
        description="Accounts of this Person return to active while the account cap allows; the rest stay deactivated."
        confirmLabel="Lift ban"
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          const r = await api.unbanPerson(target.person_id, reason);
          setNotice(
            `Ban lifted: ${r.accounts_reactivated} account(s) reactivated, ${r.accounts_left_deactivated} left deactivated.`,
          );
          setTarget(null);
          paged.reload();
        }}
      />
    </>
  );
}
