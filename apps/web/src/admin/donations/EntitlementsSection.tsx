// Admin > Donations > Entitlements (DON-ADM-004): grants from invoices, staff and the legacy download table;
// manual grant and revoke, each with a reason that goes to the access log.
import { Badge, Button, EmptyState, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { usd } from '../../support/api';
import { LoadMore, ReasonDialog, SectionHeader, fmtMs, usePaged } from '../ui';
import * as api from './api';
import './donations.css';

function describe(g: api.Grant): string {
  if (g.kind === 'patron') {
    if (g.permanent) return 'Patron, no end date';
    if (g.days) return `Patron, ${g.days} days`;
    if (g.ends_at_ms) return `Patron until ${fmtMs(g.ends_at_ms)}`;
    return 'Patron';
  }
  return `Server pool, ${usd(g.usd_cents ?? 0)}`;
}

export function EntitlementsSection() {
  const [kind, setKind] = useState<'' | 'patron' | 'server_pool'>('');
  const [activeOnly, setActiveOnly] = useState(true);
  const [target, setTarget] = useState<api.Grant | null>(null);
  const [granting, setGranting] = useState(false);
  const [gKind, setGKind] = useState<'patron' | 'server_pool'>('patron');
  const [who, setWho] = useState('');
  const [days, setDays] = useState('30');
  const [cents, setCents] = useState('1000');
  const [forever, setForever] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const paged = usePaged<api.Grant>(`grants:${kind}:${activeOnly}`, async (before) => {
    const p = await api.listGrants({ kind, active: activeOnly || undefined, before });
    return { items: p.items, next: p.next_before };
  });

  const validGrant =
    /^\d+$/.test(who.trim()) &&
    (gKind === 'patron' ? forever || /^\d+$/.test(days.trim()) : /^\d+$/.test(cents.trim()));

  return (
    <>
      <SectionHeader
        title="Entitlements"
        description="What gives members Patron perks and servers their level. Revoking takes effect at once."
        actions={<Button onClick={() => setGranting(true)}>Grant</Button>}
      />
      <div className="ss-toolbar">
        <label>
          Kind{' '}
          <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
            <option value="">All</option>
            <option value="patron">Patron</option>
            <option value="server_pool">Server pool</option>
          </select>
        </label>
        <label>
          <input
            type="checkbox"
            checked={activeOnly}
            onChange={(e) => setActiveOnly(e.target.checked)}
          />{' '}
          Only active
        </label>
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
        <EmptyState title="No entitlements" description="Nothing matches." />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label="Entitlements">
          {paged.items.map((g) => (
            <li key={g.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {describe(g)}{' '}
                  <Badge variant={g.revoked_at_ms ? 'neutral' : 'success'}>
                    {g.revoked_at_ms ? 'revoked' : 'active'}
                  </Badge>{' '}
                  <Badge variant="neutral">{g.source}</Badge>
                </div>
                <div className="ss-meta">
                  {g.user_id ? `Account ${g.user_id}` : `Server ${g.guild_id}`} · from{' '}
                  {fmtMs(g.starts_at_ms)}
                  {g.reason ? ` · ${g.reason}` : ''}
                </div>
              </div>
              {!g.revoked_at_ms && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`Revoke ${describe(g)} for ${g.user_id ?? g.guild_id}`}
                  onClick={() => setTarget(g)}
                >
                  Revoke
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />

      <ReasonDialog
        open={!!target}
        title="Revoke this entitlement"
        danger
        confirmLabel="Revoke"
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.revokeGrant(target.id, reason);
          setNotice('Entitlement revoked.');
          setTarget(null);
          paged.reload();
        }}
      />
      <ReasonDialog
        open={granting}
        title="Grant an entitlement"
        confirmLabel="Grant"
        valid={validGrant}
        onCancel={() => setGranting(false)}
        onConfirm={async (reason) => {
          await api.grant({
            kind: gKind,
            ...(gKind === 'patron' ? { user_id: who.trim() } : { guild_id: who.trim() }),
            ...(gKind === 'patron'
              ? forever
                ? { permanent: true }
                : { days: Number(days) }
              : { usd_cents: Number(cents) }),
            reason,
          });
          setNotice('Entitlement granted.');
          setGranting(false);
          setWho('');
          paged.reload();
        }}
      >
        <label>
          Kind{' '}
          <select value={gKind} onChange={(e) => setGKind(e.target.value as typeof gKind)}>
            <option value="patron">Patron time for an account</option>
            <option value="server_pool">Pool contribution for a server</option>
          </select>
        </label>
        <TextInput
          label={gKind === 'patron' ? 'Account id' : 'Server id'}
          inputMode="numeric"
          value={who}
          onChange={(e) => setWho(e.target.value)}
        />
        {gKind === 'patron' ? (
          <>
            <label>
              <input
                type="checkbox"
                checked={forever}
                onChange={(e) => setForever(e.target.checked)}
              />{' '}
              No end date
            </label>
            {!forever && (
              <TextInput
                label="Days"
                inputMode="numeric"
                value={days}
                onChange={(e) => setDays(e.target.value)}
              />
            )}
          </>
        ) : (
          <TextInput
            label="Contribution (USD cents)"
            inputMode="numeric"
            value={cents}
            onChange={(e) => setCents(e.target.value)}
          />
        )}
      </ReasonDialog>
    </>
  );
}
