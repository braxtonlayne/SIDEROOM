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
        title={`Invoice ${id}`}
        size="lg"
        footer={<Button onClick={onClose}>Close</Button>}
      >
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
        {!inv && !error && <p role="status">Loading...</p>}
        {inv && (
          <>
            <dl className="adm-dl">
              <dt>Payer</dt>
              <dd>
                {inv.user_name ?? 'unknown'} <span className="adm-mono">({inv.user_id})</span>
              </dd>
              <dt>For</dt>
              <dd>
                {PURPOSE_COPY[inv.purpose]}, {usd(inv.usd_cents)} = {sats(inv.sats)}
              </dd>
              <dt>State</dt>
              <dd>
                <StateBadge state={inv.state} /> {STATE_COPY[inv.state]}{' '}
                {inv.late && <Badge variant="warning">late</Badge>}
              </dd>
              <dt>Received</dt>
              <dd>
                {sats(inv.received_sats)} ({sats(inv.confirmed_sats)} confirmed)
                {inv.shortfall_sats > 0 && ` · short by ${sats(inv.shortfall_sats)}`}
                {inv.excess_sats > 0 && ` · extra ${sats(inv.excess_sats)}`}
              </dd>
              <dt>Address</dt>
              <dd className="adm-mono">
                {inv.address} (index {inv.derivation_index})
              </dd>
              <dt>Created</dt>
              <dd>
                {fmtMs(inv.created_at_ms)}, expires {fmtMs(inv.expires_at_ms)}
              </dd>
              {inv.resolution && (
                <>
                  <dt>Resolution</dt>
                  <dd>
                    {inv.resolution.replace('_', ' ')}: {inv.resolution_note}
                  </dd>
                </>
              )}
            </dl>
            <h3>Transactions</h3>
            {inv.txs.length === 0 ? (
              <p className="ss-meta">None seen.</p>
            ) : (
              <ul className="ss-list">
                {inv.txs.map((t) => (
                  <li key={t.txid} className="ss-row">
                    <span className="ss-grow adm-mono">{t.txid}</span>
                    <span>
                      {sats(t.amount_sats)} · {t.dropped ? 'dropped' : `${t.confirmations} conf`}
                    </span>
                  </li>
                ))}
              </ul>
            )}
            <h3>Audit trail</h3>
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
                  Resolve by hand
                </Button>
              </div>
            )}
          </>
        )}
      </Modal>
      <ReasonDialog
        open={resolving}
        title="Resolve this invoice"
        description="No funds move from here. Mark paid credits the purpose once; refund noted records a refund you made in your own wallet and takes any credit back."
        confirmLabel="Resolve"
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
          Resolution{' '}
          <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
            <option value="mark_paid">Mark paid</option>
            <option value="refund_noted">Refund noted</option>
            <option value="dismiss">Dismiss</option>
          </select>
        </label>
        <TextArea
          label="Note (what was done, with any reference)"
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
        title="Invoices"
        description="Every invoice, newest first. Underpaid, overpaid and late payments need a decision."
      />
      <form
        className="ss-toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          setApplied(q.trim());
        }}
      >
        <label>
          State{' '}
          <select value={state} onChange={(e) => setState(e.target.value as InvoiceState | '')}>
            <option value="">Any state</option>
            {STATES.map((s) => (
              <option key={s} value={s}>
                {STATE_COPY[s]}
              </option>
            ))}
          </select>
        </label>
        <label>
          Purpose{' '}
          <select value={purpose} onChange={(e) => setPurpose(e.target.value as Purpose | '')}>
            <option value="">Any purpose</option>
            {PURPOSES.map((p) => (
              <option key={p} value={p}>
                {PURPOSE_COPY[p]}
              </option>
            ))}
          </select>
        </label>
        <TextInput
          label="Search"
          hint="Invoice id, address or transaction id"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <label>
          <input
            type="checkbox"
            checked={attention}
            onChange={(e) => setAttention(e.target.checked)}
          />{' '}
          Needs attention
        </label>
        <Button type="submit" variant="secondary">
          Search
        </Button>
      </form>
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState title="No invoices" description="Nothing matches these filters." />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table">
            <caption className="us-sr">Invoices</caption>
            <thead>
              <tr>
                <th scope="col">Invoice</th>
                <th scope="col">Payer</th>
                <th scope="col">For</th>
                <th scope="col">Amount</th>
                <th scope="col">State</th>
                <th scope="col">Created</th>
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
                  <td>{i.user_name ?? i.user_id}</td>
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
