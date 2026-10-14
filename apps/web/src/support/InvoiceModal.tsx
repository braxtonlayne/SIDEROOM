// The invoice dialog (DON-UI-002): amount in USD, sats and BTC, the address with a copy button, a BIP-21 QR
// code (drawn in the browser by @sideroom/ui), a countdown, live status steps, an explorer link and an
// "I've paid" check. Status arrives as SR_PAYMENT_UPDATE over the gateway, with polling as the fallback.
import { Badge, Button, Modal, Notice, QRCode, useToast } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { formatDateTime, t, useT } from '../i18n';
import { onDispatch } from '../store/events';
import * as api from './api';
import { errorText, sats, usd, type Invoice } from './api';

const POLL_MS = 15_000;

function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), everyMs);
    return () => clearInterval(timer);
  }, [everyMs]);
  return now;
}

/** Keeps one invoice fresh: gateway hint, polling while the chain can still change it, manual check. */
export function useLiveInvoice(initial: Invoice) {
  const t = useT();
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
      if (live.current) setError(errorText(e, t('support.invoice.refreshError')));
    }
  }, [id, t]);

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
    const timer = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(timer);
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
      if (live.current) setError(errorText(e, t('support.invoice.checkError')));
    } finally {
      if (live.current) setChecking(false);
    }
  }, [id, t]);

  return { invoice, setInvoice, error, checking, check, refresh };
}

const STEPS = [
  'support.invoice.stepWaiting',
  'support.invoice.stepMempool',
  'support.invoice.stepConfirming',
  'support.invoice.stepPaid',
] as const;

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
  const t = useT();
  const at = stepIndex(inv);
  return (
    <ol className="sup-steps" aria-label={t('support.invoice.progressAria')}>
      {STEPS.map((label, i) => (
        <li
          key={label}
          className={i < at ? 'done' : i === at ? 'now' : ''}
          aria-current={i === at ? 'step' : undefined}
        >
          {t(label)}
          {i === 2 && inv.state === 'confirming' && inv.confirmations !== null
            ? ` ${t('support.invoice.confirmProgress', {
                done: inv.confirmations,
                total: inv.confirmations_required,
              })}`
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
      return t('support.invoice.purposePatron', { months: inv.months });
    case 'server_level':
      return t('support.invoice.purposeServer');
    case 'invite_credit':
      return t('support.invoice.purposeCredits', { credits: inv.credits });
    default:
      return t('support.invoice.purposeDonation');
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
  const t = useT();
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
      toast.show({ title: t('support.invoice.addressCopied'), duration: 2000 });
    } catch {
      toast.show({ title: t('support.invoice.copyFailed'), duration: 4000 });
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
      setCancelError(errorText(e, t('support.invoice.cancelError')));
    } finally {
      setBusy(false);
    }
  };

  const paid = api.isPaid(invoice.state) || invoice.resolution === 'mark_paid';
  const waiting = invoice.state === 'open' || invoice.state === 'underpaid';
  const remaining = invoice.expires_at_ms - now;
  const tx = invoice.txs.find((x) => !x.dropped);
  const title = paid ? t('support.invoice.titlePaid') : t('support.invoice.titlePay');

  return (
    <Modal
      open
      onClose={onClose}
      title={title}
      description={t('support.invoice.description', {
        purpose: purposeLine(invoice),
        amount: usd(invoice.usd_cents),
      })}
      size="md"
      footer={
        <>
          {invoice.state === 'open' && invoice.received_sats === 0 && (
            <Button variant="secondary" onClick={() => void cancel()} disabled={busy}>
              {t('support.invoice.cancelInvoice')}
            </Button>
          )}
          <Button onClick={onClose}>
            {paid ? t('support.invoice.done') : t('support.invoice.close')}
          </Button>
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
          {t('support.invoice.rateLocked', {
            price: usd(invoice.rate.cents_per_btc),
            source: invoice.rate.source,
          })}
          {invoice.rate.stale && ` ${t('support.invoice.rateStale')}`}
        </p>

        {!paid && invoice.state !== 'cancelled' && (
          <>
            <div className="sup-pay">
              <QRCode
                value={invoice.bip21}
                label={t('support.invoice.qrLabel', {
                  amount: sats(invoice.sats),
                  address: invoice.address,
                })}
                size={176}
              />
              <div className="sup-address">
                <label htmlFor="sup-address-field">{t('support.invoice.sendExactly')}</label>
                <code id="sup-address-field" className="sup-code" data-testid="invoice-address">
                  {invoice.address}
                </code>
                <div className="sup-actions">
                  <Button size="sm" variant="secondary" onClick={() => void copy()}>
                    {t('support.invoice.copyAddress')}
                  </Button>
                  <a className="sup-link" href={invoice.bip21}>
                    {t('support.invoice.openWallet')}
                  </a>
                </div>
                <p className="sup-muted">
                  {t('support.invoice.addressNote')} <strong>{invoice.btc} BTC</strong>
                </p>
              </div>
            </div>
            {invoice.state === 'open' &&
              (remaining > 0 ? (
                <p role="timer" className="sup-muted">
                  {t('support.invoice.expiresIn')} <strong>{mmss(remaining)}</strong>
                </p>
              ) : (
                <Notice variant="warning" title={t('support.invoice.expiredTitle')}>
                  {t('support.invoice.expiredBody')}
                </Notice>
              ))}
          </>
        )}

        <Steps inv={invoice} />

        {invoice.state === 'underpaid' && (
          <Notice variant="warning" title={t('support.invoice.underTitle')}>
            {t('support.invoice.underBody', {
              received: sats(invoice.received_sats),
              shortfall: sats(invoice.shortfall_sats),
            })}
          </Notice>
        )}
        {invoice.state === 'overpaid' && (
          <Notice variant="info" title={t('support.invoice.overTitle')}>
            {t('support.invoice.overBody', {
              purpose: purposeLine(invoice).toLowerCase(),
              excess: sats(invoice.excess_sats),
            })}
          </Notice>
        )}
        {invoice.state === 'late_paid' && (
          <Notice variant="info" title={t('support.invoice.lateTitle')}>
            {t('support.invoice.lateBody')}
          </Notice>
        )}
        {invoice.state === 'expired' && invoice.received_sats === 0 && (
          <Notice variant="warning" title={t('support.invoice.noneTitle')}>
            {t('support.invoice.noneBody')}
          </Notice>
        )}
        {waiting || invoice.state === 'seen' || invoice.state === 'confirming' ? (
          <p className="sup-muted">{t('support.invoice.tip')}</p>
        ) : null}

        {(tx || invoice.explorer_url) && (
          <p className="sup-muted">
            {tx && (
              <>
                {t('support.invoice.transaction')}{' '}
                <code className="sup-code-inline">{tx.txid.slice(0, 16)}…</code>{' '}
              </>
            )}
            {invoice.explorer_url && (
              <a className="sup-link" href={invoice.explorer_url} target="_blank" rel="noreferrer">
                {t('support.invoice.explorer')}
              </a>
            )}
          </p>
        )}
        {paid && (
          <dl className="sup-receipt" aria-label={t('support.invoice.receiptAria')}>
            <div>
              <dt>{t('support.invoice.receipt')}</dt>
              <dd>#{invoice.id}</dd>
            </div>
            <div>
              <dt>{t('support.invoice.paid')}</dt>
              <dd>
                {t('support.invoice.paidFor', {
                  amount: sats(invoice.confirmed_sats || invoice.received_sats),
                  total: usd(invoice.usd_cents),
                })}
              </dd>
            </div>
            {invoice.credited_at_ms && (
              <div>
                <dt>{t('support.invoice.credited')}</dt>
                <dd>{formatDateTime(invoice.credited_at_ms)}</dd>
              </div>
            )}
          </dl>
        )}

        {(waiting || invoice.state === 'seen' || invoice.state === 'confirming') && (
          <div className="sup-actions">
            <Button variant="secondary" onClick={() => void check()} disabled={checking}>
              {checking ? t('support.invoice.checking') : t('support.invoice.ivePaid')}
            </Button>
            <span className="sup-muted" role="status">
              {t('support.invoice.autoUpdates')}
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
