// The invoice dialog (DON-UI-002): amount in USD, sats and BTC, the address with a copy button, a BIP-21 QR
// code (drawn in the browser by @sideroom/ui), a countdown, live status steps, an explorer link and an
// "I've paid" check. Status arrives as SR_PAYMENT_UPDATE over the gateway, with polling as the fallback.
import { Badge, Button, Modal, Notice, QRCode, useToast } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { onDispatch } from '../store/events';
import * as api from './api';
import { errorText, sats, usd, type Invoice } from './api';

const POLL_MS = 15_000;

function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), everyMs);
    return () => clearInterval(t);
  }, [everyMs]);
  return now;
}

/** Keeps one invoice fresh: gateway hint, polling while the chain can still change it, manual check. */
export function useLiveInvoice(initial: Invoice) {
  const [invoice, setInvoice] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const id = initial.id;

  const refresh = useCallback(async () => {
    try {
      const next = await api.getInvoice(id);
      if (live.current) {
        setInvoice(next);
        setError(null);
      }
    } catch (e) {
      if (live.current) setError(errorText(e, 'Could not refresh the status.'));
    }
  }, [id]);

  useEffect(
    () =>
      onDispatch((event, data) => {
        if (event === 'SR_PAYMENT_UPDATE' && (data as { invoice_id?: string })?.invoice_id === id)
          void refresh();
      }),
    [id, refresh],
  );

  const liveState = api.isLive(invoice.state);
  useEffect(() => {
    if (!liveState) return;
    const t = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(t);
  }, [liveState, refresh]);

  const check = useCallback(async () => {
    setChecking(true);
    try {
      const next = await api.checkInvoice(id);
      if (live.current) {
        setInvoice(next);
        setError(null);
      }
    } catch (e) {
      if (live.current) setError(errorText(e, 'Could not check the payment.'));
    } finally {
      if (live.current) setChecking(false);
    }
  }, [id]);

  return { invoice, setInvoice, error, checking, check, refresh };
}

const STEPS = ['Waiting for payment', 'Seen in the mempool', 'Confirming', 'Paid'] as const;

function stepIndex(inv: Invoice): number {
  switch (inv.state) {
    case 'seen':
      return 1;
    case 'confirming':
      return 2;
    case 'confirmed':
    case 'overpaid':
    case 'late_paid':
    case 'resolved':
      return 3;
    default:
      return 0;
  }
}

function Steps({ inv }: { inv: Invoice }) {
  const at = stepIndex(inv);
  return (
    <ol className="sup-steps" aria-label="Payment progress">
      {STEPS.map((label, i) => (
        <li
          key={label}
          className={i < at ? 'done' : i === at ? 'now' : ''}
          aria-current={i === at ? 'step' : undefined}
        >
          {label}
          {i === 2 && inv.state === 'confirming' && inv.confirmations !== null
            ? ` (${inv.confirmations} of ${inv.confirmations_required})`
            : ''}
        </li>
      ))}
    </ol>
  );
}

function mmss(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

export function purposeLine(inv: Invoice): string {
  switch (inv.purpose) {
    case 'patron':
      return `Patron, ${inv.months} month${inv.months === 1 ? '' : 's'}`;
    case 'server_level':
      return 'Support for a server';
    case 'invite_credit':
      return `${inv.credits} invite credit${inv.credits === 1 ? '' : 's'}`;
    default:
      return 'Donation to the community';
  }
}

export function InvoiceModal({
  invoice: initial,
  onClose,
  onChanged,
}: {
  invoice: Invoice;
  onClose: () => void;
  /** Called after the invoice changed so the page can refresh its history and status. */
  onChanged?: (inv: Invoice) => void;
}) {
  const toast = useToast();
  const { invoice, setInvoice, error, checking, check } = useLiveInvoice(initial);
  const now = useNow(1000);
  const [busy, setBusy] = useState(false);
  const [cancelError, setCancelError] = useState<string | null>(null);
  const last = useRef(invoice.state + invoice.received_sats);
  useEffect(() => {
    const key = invoice.state + invoice.received_sats;
    if (key !== last.current) {
      last.current = key;
      onChanged?.(invoice);
    }
  }, [invoice, onChanged]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(invoice.address);
      toast.show({ title: 'Address copied', duration: 2000 });
    } catch {
      toast.show({ title: 'Copy failed. Select the address and copy it by hand.', duration: 4000 });
    }
  };
  const cancel = async () => {
    setBusy(true);
    setCancelError(null);
    try {
      const next = await api.cancelInvoice(invoice.id);
      setInvoice(next);
      onChanged?.(next);
    } catch (e) {
      setCancelError(errorText(e, 'Could not cancel the invoice.'));
    } finally {
      setBusy(false);
    }
  };

  const paid = api.isPaid(invoice.state) || invoice.resolution === 'mark_paid';
  const waiting = invoice.state === 'open' || invoice.state === 'underpaid';
  const remaining = invoice.expires_at_ms - now;
  const tx = invoice.txs.find((t) => !t.dropped);
  const title = paid ? 'Payment received' : 'Pay with Bitcoin';

  return (
    <Modal
      open
      onClose={onClose}
      title={title}
      description={`${purposeLine(invoice)}. ${usd(invoice.usd_cents)}.`}
      size="md"
      footer={
        <>
          {invoice.state === 'open' && invoice.received_sats === 0 && (
            <Button variant="secondary" onClick={() => void cancel()} disabled={busy}>
              Cancel invoice
            </Button>
          )}
          <Button onClick={onClose}>{paid ? 'Done' : 'Close'}</Button>
        </>
      }
    >
      <div className="sup-invoice">
        <div className="sup-amounts">
          <div>
            <span className="sup-big">{usd(invoice.usd_cents)}</span>
            <span className="sup-muted">
              {sats(invoice.sats)} ({invoice.btc} BTC)
            </span>
          </div>
          <Badge variant={paid ? 'success' : waiting ? 'accent' : 'neutral'}>
            {api.STATE_COPY[invoice.state]}
          </Badge>
        </div>
        <p className="sup-muted">
          Rate locked at {usd(invoice.rate.cents_per_btc)} per BTC ({invoice.rate.source}).
          {invoice.rate.stale &&
            ' The price sources were unreachable, so the last known price was used.'}
        </p>

        {!paid && invoice.state !== 'cancelled' && (
          <>
            <div className="sup-pay">
              <QRCode
                value={invoice.bip21}
                label={`QR code to pay ${sats(invoice.sats)} to ${invoice.address}`}
                size={176}
              />
              <div className="sup-address">
                <label htmlFor="sup-address-field">Send exactly this amount to</label>
                <code id="sup-address-field" className="sup-code" data-testid="invoice-address">
                  {invoice.address}
                </code>
                <div className="sup-actions">
                  <Button size="sm" variant="secondary" onClick={() => void copy()}>
                    Copy address
                  </Button>
                  <a className="sup-link" href={invoice.bip21}>
                    Open in wallet
                  </a>
                </div>
                <p className="sup-muted">
                  This address is for this invoice only; do not reuse it. Amount:{' '}
                  <strong>{invoice.btc} BTC</strong>
                </p>
              </div>
            </div>
            {invoice.state === 'open' &&
              (remaining > 0 ? (
                <p role="timer" className="sup-muted">
                  Expires in <strong>{mmss(remaining)}</strong>
                </p>
              ) : (
                <Notice variant="warning" title="This invoice has expired">
                  If you already sent the payment it is still credited when it confirms, and staff
                  are told it came late. Otherwise create a new invoice.
                </Notice>
              ))}
          </>
        )}

        <Steps inv={invoice} />

        {invoice.state === 'underpaid' && (
          <Notice variant="warning" title="Not enough was received">
            We saw {sats(invoice.received_sats)}. Send {sats(invoice.shortfall_sats)} more to the
            same address to complete it; staff can also sort it out by hand.
          </Notice>
        )}
        {invoice.state === 'overpaid' && (
          <Notice variant="info" title="You sent more than needed">
            Your {purposeLine(invoice).toLowerCase()} is active. The extra{' '}
            {sats(invoice.excess_sats)} is noted for staff; they will contact you if it is to be
            returned.
          </Notice>
        )}
        {invoice.state === 'late_paid' && (
          <Notice variant="info" title="It arrived after the invoice expired">
            It has still been credited. Staff were told it came late.
          </Notice>
        )}
        {invoice.state === 'expired' && invoice.received_sats === 0 && (
          <Notice variant="warning" title="No payment arrived in time">
            Create a new invoice to try again. A payment that is still on its way will be credited
            when it confirms.
          </Notice>
        )}
        {waiting || invoice.state === 'seen' || invoice.state === 'confirming' ? (
          <p className="sup-muted">
            Tip: a normal network fee confirms in the next block or two. Very low fees can wait for
            hours.
          </p>
        ) : null}

        {(tx || invoice.explorer_url) && (
          <p className="sup-muted">
            {tx && (
              <>
                Transaction <code className="sup-code-inline">{tx.txid.slice(0, 16)}…</code>{' '}
              </>
            )}
            {invoice.explorer_url && (
              <a className="sup-link" href={invoice.explorer_url} target="_blank" rel="noreferrer">
                View on a block explorer
              </a>
            )}
          </p>
        )}
        {paid && (
          <dl className="sup-receipt" aria-label="Receipt">
            <div>
              <dt>Receipt</dt>
              <dd>#{invoice.id}</dd>
            </div>
            <div>
              <dt>Paid</dt>
              <dd>
                {sats(invoice.confirmed_sats || invoice.received_sats)} for {usd(invoice.usd_cents)}
              </dd>
            </div>
            {invoice.credited_at_ms && (
              <div>
                <dt>Credited</dt>
                <dd>{new Date(invoice.credited_at_ms).toLocaleString()}</dd>
              </div>
            )}
          </dl>
        )}

        {(waiting || invoice.state === 'seen' || invoice.state === 'confirming') && (
          <div className="sup-actions">
            <Button variant="secondary" onClick={() => void check()} disabled={checking}>
              {checking ? 'Checking...' : "I've paid"}
            </Button>
            <span className="sup-muted" role="status">
              This page updates by itself.
            </span>
          </div>
        )}
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
        {cancelError && (
          <Notice variant="danger" role="alert">
            {cancelError}
          </Notice>
        )}
      </div>
    </Modal>
  );
}
