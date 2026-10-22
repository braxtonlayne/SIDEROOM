// Admin > Donations > Invoices (DON-ADM-003): search, detail with transactions and audit trail, and manual
// resolution. Staff never move funds from here: "refund noted" only records a refund the operator made in
// their own wallet.
import { Badge, Button, EmptyState, Modal, Notice, TextArea, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import {
  PURPOSE_COPY,
  STATE_COPY,
  sats,
  usd,
  type InvoiceState,
  type Purpose,
} from '../../support/api';
import { useT } from '../../i18n';
import { LoadMore, ReasonDialog, SectionHeader, StateBadge, fmtMs, usePaged } from '../ui';
import * as api from './api';
import './donations.css';

const STATES: InvoiceState[] = [
  'open',
  'seen',
  'confirming',
  'confirmed',
  'overpaid',
  'late_paid',
  'underpaid',
  'expired',
  'cancelled',
  'resolved',
];
const PURPOSES: Purpose[] = ['donate', 'patron', 'server_level', 'invite_credit'];

function Detail({
  id,
  onClose,
  onChanged,
}: {
  id: string;
  onClose: () => void;
  onChanged: () => void;
}) {
  const t = useT();
  const [inv, setInv] = useState<api.AdminInvoice | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [resolving, setResolving] = useState(false);
  const [kind, setKind] = useState<'mark_paid' | 'refund_noted' | 'dismiss'>('mark_paid');
  const [note, setNote] = useState('');
  useEffect(() => {
    let live = true;
    api
      .getInvoice(id)
      .then((v) => live && setInv(v))
      .catch((e: unknown) => live && setError(api.errorText(e)));
    return () => {
      live = false;
    };
  }, [id]);
  const open = inv && inv.state !== 'cancelled' && inv.state !== 'resolved';
  return (
    <>
      <Modal
        open
        onClose={onClose}
        title={t('admin.donations.invoices.detailTitle', { id })}
        size="lg"
        footer={<Button onClick={onClose}>{t('admin.donations.invoices.close')}</Button>}
      >
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
        {!inv && !error && <p role="status">{t('admin.donations.invoices.loading')}</p>}
        {inv && (
          <>
            <dl className="adm-dl">
              <dt>{t('admin.donations.invoices.dtPayer')}</dt>
              <dd>
                {inv.user_id === '0' ? (
                  t('admin.donations.invoices.detached')
                ) : (
                  <>
                    {inv.user_name ?? t('admin.donations.invoices.unknown')}{' '}
                    <span className="adm-mono">({inv.user_id})</span>
                  </>
                )}
              </dd>
              <dt>{t('admin.donations.invoices.dtFor')}</dt>
              <dd>
                {t('admin.donations.invoices.forValue', {
                  purpose: PURPOSE_COPY[inv.purpose],
                  usd: usd(inv.usd_cents),
                  sats: sats(inv.sats),
                })}
              </dd>
              <dt>{t('admin.donations.invoices.dtState')}</dt>
              <dd>
                <StateBadge state={inv.state} /> {STATE_COPY[inv.state]}{' '}
                {inv.late && <Badge variant="warning">{t('admin.donations.invoices.late')}</Badge>}
              </dd>
              <dt>{t('admin.donations.invoices.dtReceived')}</dt>
              <dd>
                {t('admin.donations.invoices.receivedValue', {
                  received: sats(inv.received_sats),
                  confirmed: sats(inv.confirmed_sats),
                })}
                {inv.shortfall_sats > 0 &&
                  ` · ${t('admin.donations.invoices.shortBy', { amount: sats(inv.shortfall_sats) })}`}
                {inv.excess_sats > 0 &&
                  ` · ${t('admin.donations.invoices.extra', { amount: sats(inv.excess_sats) })}`}
              </dd>
              <dt>{t('admin.donations.invoices.dtAddress')}</dt>
              <dd className="adm-mono">
                {t('admin.donations.invoices.addressValue', {
                  address: inv.address,
                  index: inv.derivation_index,
                })}
              </dd>
              <dt>{t('admin.donations.invoices.dtCreated')}</dt>
              <dd>
                {t('admin.donations.invoices.createdValue', {
                  created: fmtMs(inv.created_at_ms),
                  expires: fmtMs(inv.expires_at_ms),
                })}
              </dd>
              {inv.resolution && (
                <>
                  <dt>{t('admin.donations.invoices.dtResolution')}</dt>
                  <dd>
                    {inv.resolution.replace('_', ' ')}: {inv.resolution_note}
                  </dd>
                </>
              )}
            </dl>
            <h3>{t('admin.donations.invoices.transactions')}</h3>
            {inv.txs.length === 0 ? (
              <p className="ss-meta">{t('admin.donations.invoices.noneSeen')}</p>
            ) : (
              <ul className="ss-list">
                {inv.txs.map((tx) => (
                  <li key={tx.txid} className="ss-row">
                    <span className="ss-grow adm-mono">{tx.txid}</span>
                    <span>
                      {sats(tx.amount_sats)} ·{' '}
                      {tx.dropped
                        ? t('admin.donations.invoices.dropped')
                        : t('admin.donations.invoices.confirmations', { count: tx.confirmations })}
                    </span>
                  </li>
                ))}
              </ul>
            )}
            <h3>{t('admin.donations.invoices.auditTrail')}</h3>
            <ul className="adm-audit">
              {inv.audit.map((a, i) => (
                <li key={i}>
                  {fmtMs(a.at_ms)} · {a.kind.replace('_', ' ')} · {JSON.stringify(a.detail)}
                </li>
              ))}
            </ul>
            {open && (
              <div className="adm-actions">
                <Button variant="secondary" onClick={() => setResolving(true)}>
                  {t('admin.donations.invoices.resolveByHand')}
                </Button>
              </div>
            )}
          </>
        )}
      </Modal>
      <ReasonDialog
        open={resolving}
        title={t('admin.donations.invoices.resolveTitle')}
        description={t('admin.donations.invoices.resolveDescription')}
        confirmLabel={t('admin.donations.invoices.resolve')}
        valid={note.trim().length > 0}
        onCancel={() => setResolving(false)}
        onConfirm={async (reason) => {
          const r = await api.resolveInvoice(id, { resolution: kind, note: note.trim(), reason });
          setInv(r);
          setResolving(false);
          setNote('');
          onChanged();
        }}
      >
        <label>
          {t('admin.donations.invoices.resolution')}{' '}
          <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
            <option value="mark_paid">{t('admin.donations.invoices.markPaid')}</option>
            <option value="refund_noted">{t('admin.donations.invoices.refundNoted')}</option>
            <option value="dismiss">{t('admin.donations.invoices.dismiss')}</option>
          </select>
        </label>
        <TextArea
          label={t('admin.donations.invoices.noteLabel')}
          value={note}
          rows={3}
          maxLength={1000}
          onChange={(e) => setNote(e.target.value)}
        />
      </ReasonDialog>
    </>
  );
}

export function InvoicesSection() {
  const t = useT();
  const [state, setState] = useState<InvoiceState | ''>('');
  const [purpose, setPurpose] = useState<Purpose | ''>('');
  const [attention, setAttention] = useState(false);
  const [q, setQ] = useState('');
  const [applied, setApplied] = useState('');
  const [open, setOpen] = useState<string | null>(null);
  const paged = usePaged<api.AdminInvoice>(
    `inv:${state}:${purpose}:${attention}:${applied}`,
    async (before) => {
      const p = await api.listInvoices({
        state,
        purpose,
        q: applied,
        needs_attention: attention || undefined,
        before,
        limit: 25,
      });
      return { items: p.items, next: p.next_before };
    },
  );
  return (
    <>
      <SectionHeader
        title={t('admin.donations.invoices.title')}
        description={t('admin.donations.invoices.description')}
      />
      <form
        className="ss-toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          setApplied(q.trim());
        }}
      >
        <label>
          {t('admin.donations.invoices.filterState')}{' '}
          <select value={state} onChange={(e) => setState(e.target.value as InvoiceState | '')}>
            <option value="">{t('admin.donations.invoices.anyState')}</option>
            {STATES.map((s) => (
              <option key={s} value={s}>
                {STATE_COPY[s]}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t('admin.donations.invoices.filterPurpose')}{' '}
          <select value={purpose} onChange={(e) => setPurpose(e.target.value as Purpose | '')}>
            <option value="">{t('admin.donations.invoices.anyPurpose')}</option>
            {PURPOSES.map((p) => (
              <option key={p} value={p}>
                {PURPOSE_COPY[p]}
              </option>
            ))}
          </select>
        </label>
        <TextInput
          label={t('admin.donations.invoices.searchLabel')}
          hint={t('admin.donations.invoices.searchHint')}
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <label>
          <input
            type="checkbox"
            checked={attention}
            onChange={(e) => setAttention(e.target.checked)}
          />{' '}
          {t('admin.donations.invoices.needsAttention')}
        </label>
        <Button type="submit" variant="secondary">
          {t('admin.donations.invoices.search')}
        </Button>
      </form>
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.donations.invoices.emptyTitle')}
          description={t('admin.donations.invoices.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table">
            <caption className="us-sr">{t('admin.donations.invoices.title')}</caption>
            <thead>
              <tr>
                <th scope="col">{t('admin.donations.invoices.colInvoice')}</th>
                <th scope="col">{t('admin.donations.invoices.dtPayer')}</th>
                <th scope="col">{t('admin.donations.invoices.dtFor')}</th>
                <th scope="col">{t('admin.donations.invoices.colAmount')}</th>
                <th scope="col">{t('admin.donations.invoices.dtState')}</th>
                <th scope="col">{t('admin.donations.invoices.dtCreated')}</th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((i) => (
                <tr key={i.id}>
                  <th scope="row">
                    <Button variant="link" size="sm" onClick={() => setOpen(i.id)}>
                      {i.id}
                    </Button>
                  </th>
                  <td>
                    {i.user_id === '0'
                      ? t('admin.donations.invoices.detached')
                      : (i.user_name ?? i.user_id)}
                  </td>
                  <td>{PURPOSE_COPY[i.purpose]}</td>
                  <td>{usd(i.usd_cents)}</td>
                  <td>
                    <StateBadge state={i.state} />
                  </td>
                  <td>{fmtMs(i.created_at_ms)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
      {open && <Detail id={open} onClose={() => setOpen(null)} onChanged={paged.reload} />}
    </>
  );
}
