// Admin > Donations > Entitlements (DON-ADM-004): grants from invoices, staff and the legacy download table;
// manual grant and revoke, each with a reason that goes to the access log.
import { Badge, Button, EmptyState, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { t, useT } from '../../i18n';
import { usd } from '../../support/api';
import { LoadMore, ReasonDialog, SectionHeader, fmtMs, usePaged } from '../ui';
import * as api from './api';
import './donations.css';

function describe(g: api.Grant): string {
  if (g.kind === 'patron') {
    if (g.permanent) return t('admin.entitlements.patronForever');
    if (g.days) return t('admin.entitlements.patronDays', { days: g.days });
    if (g.ends_at_ms) return t('admin.entitlements.patronUntil', { time: fmtMs(g.ends_at_ms) });
    return t('admin.entitlements.patron');
  }
  return t('admin.entitlements.serverPool', { amount: usd(g.usd_cents ?? 0) });
}

export function EntitlementsSection() {
  const t = useT();
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
        title={t('admin.entitlements.title')}
        description={t('admin.entitlements.description')}
        actions={<Button onClick={() => setGranting(true)}>{t('admin.entitlements.grant')}</Button>}
      />
      <div className="ss-toolbar">
        <label>
          {t('admin.entitlements.kind')}{' '}
          <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
            <option value="">{t('admin.entitlements.all')}</option>
            <option value="patron">{t('admin.entitlements.patron')}</option>
            <option value="server_pool">{t('admin.entitlements.serverPoolOption')}</option>
          </select>
        </label>
        <label>
          <input
            type="checkbox"
            checked={activeOnly}
            onChange={(e) => setActiveOnly(e.target.checked)}
          />{' '}
          {t('admin.entitlements.onlyActive')}
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
        <EmptyState
          title={t('admin.entitlements.empty.title')}
          description={t('admin.entitlements.empty.description')}
        />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.entitlements.title')}>
          {paged.items.map((g) => (
            <li key={g.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {describe(g)}{' '}
                  <Badge variant={g.revoked_at_ms ? 'neutral' : 'success'}>
                    {g.revoked_at_ms
                      ? t('admin.entitlements.revoked')
                      : t('admin.entitlements.active')}
                  </Badge>{' '}
                  <Badge variant="neutral">{g.source}</Badge>
                </div>
                <div className="ss-meta">
                  {g.user_id
                    ? t('admin.entitlements.account', { id: g.user_id })
                    : t('admin.entitlements.server', { id: g.guild_id ?? '' })}{' '}
                  · {t('admin.entitlements.from', { time: fmtMs(g.starts_at_ms) })}
                  {g.reason ? ` · ${g.reason}` : ''}
                </div>
              </div>
              {!g.revoked_at_ms && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.entitlements.revokeAria', {
                    what: describe(g),
                    who: g.user_id ?? g.guild_id ?? '',
                  })}
                  onClick={() => setTarget(g)}
                >
                  {t('admin.entitlements.revoke')}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />

      <ReasonDialog
        open={!!target}
        title={t('admin.entitlements.revokeDialog.title')}
        danger
        confirmLabel={t('admin.entitlements.revoke')}
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.revokeGrant(target.id, reason);
          setNotice(t('admin.entitlements.revokeDialog.done'));
          setTarget(null);
          paged.reload();
        }}
      />
      <ReasonDialog
        open={granting}
        title={t('admin.entitlements.grantDialog.title')}
        confirmLabel={t('admin.entitlements.grant')}
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
          setNotice(t('admin.entitlements.grantDialog.done'));
          setGranting(false);
          setWho('');
          paged.reload();
        }}
      >
        <label>
          {t('admin.entitlements.kind')}{' '}
          <select value={gKind} onChange={(e) => setGKind(e.target.value as typeof gKind)}>
            <option value="patron">{t('admin.entitlements.grantDialog.patronOption')}</option>
            <option value="server_pool">{t('admin.entitlements.grantDialog.poolOption')}</option>
          </select>
        </label>
        <TextInput
          label={
            gKind === 'patron'
              ? t('admin.entitlements.grantDialog.accountId')
              : t('admin.entitlements.grantDialog.serverId')
          }
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
              {t('admin.entitlements.grantDialog.noEnd')}
            </label>
            {!forever && (
              <TextInput
                label={t('admin.entitlements.grantDialog.days')}
                inputMode="numeric"
                value={days}
                onChange={(e) => setDays(e.target.value)}
              />
            )}
          </>
        ) : (
          <TextInput
            label={t('admin.entitlements.grantDialog.cents')}
            inputMode="numeric"
            value={cents}
            onChange={(e) => setCents(e.target.value)}
          />
        )}
      </ReasonDialog>
    </>
  );
}
