// Admin > Donations (DON-ADM-001): wallet configuration and health. The page never shows the descriptor
// back: only its short fingerprint. Pasting a seed or a private key is refused by the server.
import { Badge, Button, Notice, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { usd } from '../../support/api';
import { useT } from '../../i18n';
import { Async, ReasonDialog, SectionHeader, Stat, fmtMs, useResource } from '../ui';
import * as api from './api';
import './donations.css';

function ageText(t: ReturnType<typeof useT>, ms: number | null) {
  if (!ms) return t('admin.donations.overview.never');
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 90) return t('admin.donations.overview.secondsAgo', { n: s });
  if (s < 5400) return t('admin.donations.overview.minutesAgo', { n: Math.round(s / 60) });
  return t('admin.donations.overview.hoursAgo', { n: Math.round(s / 3600) });
}

function Health({ s }: { s: api.Status }) {
  const t = useT();
  const w = s.watcher;
  const healthy = !!w.last_ok_ms && !w.last_error;
  return (
    <section className="ss-card" aria-labelledby="don-health-h">
      <h2 id="don-health-h">{t('admin.donations.overview.health')}</h2>
      <div className="adm-stats">
        <Stat
          label={t('admin.donations.overview.donations')}
          value={s.enabled ? t('admin.donations.overview.on') : t('admin.donations.overview.off')}
        />
        <Stat
          label={t('admin.donations.overview.watcher')}
          value={
            healthy
              ? t('admin.donations.overview.healthy')
              : w.last_run_ms
                ? t('admin.donations.overview.problem')
                : t('admin.donations.overview.notRun')
          }
          hint={t('admin.donations.overview.lastOk', { age: ageText(t, w.last_ok_ms) })}
        />
        <Stat
          label={t('admin.donations.overview.chainTip')}
          value={w.tip_height ?? t('admin.donations.overview.unknown')}
        />
        <Stat
          label={t('admin.donations.overview.needsAttention')}
          value={s.needs_attention}
          hint={t('admin.donations.overview.needsAttentionHint')}
        />
      </div>
      {w.last_error && (
        <Notice
          variant="warning"
          title={t('admin.donations.overview.watcherProblem')}
          role="status"
        >
          {w.last_error}
        </Notice>
      )}
      {!s.enabled && (
        <Notice variant="info" title={t('admin.donations.overview.donationsOffTitle')}>
          {t('admin.donations.overview.donationsOff')}
        </Notice>
      )}
      {s.wallet ? (
        <dl className="adm-dl">
          <dt>{t('admin.donations.overview.wallet')}</dt>
          <dd>
            <code>{s.wallet.fingerprint}</code>{' '}
            {t('admin.donations.overview.onNetwork', { network: s.wallet.network })}
          </dd>
          <dt>{t('admin.donations.overview.nextIndex')}</dt>
          <dd>{s.wallet.next_index}</dd>
          <dt>{t('admin.donations.overview.unusedInARow')}</dt>
          <dd>
            {t('admin.donations.overview.gapValue', {
              used: s.wallet.gap_used,
              limit: s.wallet.gap_limit,
            })}{' '}
            {s.wallet.gap_used >= s.wallet.gap_limit && (
              <Badge variant="danger">{t('admin.donations.overview.gapReached')}</Badge>
            )}
          </dd>
        </dl>
      ) : (
        <p className="ss-meta">{t('admin.donations.overview.noWallet')}</p>
      )}
      <h3>{t('admin.donations.overview.invoices90')}</h3>
      <p className="ss-meta">
        {Object.entries(s.counts)
          .map(([k, n]) => `${k.replace('_', ' ')}: ${n}`)
          .join(' · ') || t('admin.donations.overview.noneYet')}
      </p>
    </section>
  );
}

function Rates({ s }: { s: api.Status }) {
  const t = useT();
  return (
    <section className="ss-card" aria-labelledby="don-rates-h">
      <h2 id="don-rates-h">{t('admin.donations.overview.priceSources')}</h2>
      {s.rate ? (
        <p>
          {t('admin.donations.overview.current')} <strong>{usd(s.rate.cents_per_btc)}</strong>{' '}
          {t('admin.donations.overview.perBtc', { source: s.rate.source })}
          {s.rate.stale && <Badge variant="warning">{t('admin.donations.overview.stale')}</Badge>}
        </p>
      ) : (
        <Notice variant="warning" title={t('admin.donations.overview.noPriceTitle')} role="status">
          {t('admin.donations.overview.noPrice')}
        </Notice>
      )}
      {s.rate_sources.length > 0 && (
        <table className="adm-table">
          <caption className="us-sr">{t('admin.donations.overview.priceSources')}</caption>
          <thead>
            <tr>
              <th scope="col">{t('admin.donations.overview.colSource')}</th>
              <th scope="col">{t('admin.donations.overview.colLastPrice')}</th>
              <th scope="col">{t('admin.donations.overview.colLastOk')}</th>
              <th scope="col">{t('admin.donations.overview.colError')}</th>
            </tr>
          </thead>
          <tbody>
            {s.rate_sources.map((r) => (
              <tr key={r.name}>
                <th scope="row">{r.name}</th>
                <td>{r.last_cents ? usd(r.last_cents) : ''}</td>
                <td>{r.last_ok_ms ? fmtMs(r.last_ok_ms) : t('admin.donations.overview.never')}</td>
                <td>{r.last_error ?? ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

function WalletForm({ onSaved }: { onSaved: () => void }) {
  const t = useT();
  const [descriptor, setDescriptor] = useState('');
  const [network, setNetwork] = useState('mainnet');
  const [gap, setGap] = useState('20');
  const [addresses, setAddresses] = useState<string[] | null>(null);
  const [fingerprint, setFingerprint] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

  const test = async () => {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      const r = await api.testWallet(descriptor, network);
      setAddresses(r.addresses);
      setFingerprint(r.fingerprint);
    } catch (e) {
      setAddresses(null);
      setError(api.errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="ss-card" aria-labelledby="don-wallet-h">
      <h2 id="don-wallet-h">{t('admin.donations.overview.wallet')}</h2>
      <Notice variant="info" title={t('admin.donations.overview.watchOnly')}>
        {t('admin.donations.overview.pasteDescriptor')} <code>wpkh(xpub.../0/*)</code>.{' '}
        {t('admin.donations.overview.serverNeverSpends')}
      </Notice>
      <form className="adm-form" onSubmit={(e) => e.preventDefault()}>
        <TextArea
          label={t('admin.donations.overview.descriptorLabel')}
          value={descriptor}
          rows={3}
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => setDescriptor(e.target.value)}
        />
        <label>
          {t('admin.donations.overview.network')}{' '}
          <select value={network} onChange={(e) => setNetwork(e.target.value)}>
            <option value="mainnet">mainnet</option>
            <option value="testnet">testnet</option>
            <option value="signet">signet</option>
            <option value="regtest">regtest</option>
          </select>
        </label>
        <TextInput
          label={t('admin.donations.overview.gapLimit')}
          hint={t('admin.donations.overview.gapLimitHint')}
          inputMode="numeric"
          value={gap}
          onChange={(e) => setGap(e.target.value)}
        />
        <div className="adm-actions">
          <Button
            variant="secondary"
            disabled={busy || descriptor.trim() === ''}
            onClick={() => void test()}
          >
            {t('admin.donations.overview.testDerivation')}
          </Button>
          <Button disabled={!addresses} onClick={() => setConfirm(true)}>
            {t('admin.donations.overview.useThisWallet')}
          </Button>
        </div>
      </form>
      {error && (
        <Notice variant="danger" role="alert">
          {error}
        </Notice>
      )}
      {addresses && (
        <div>
          <p>
            {t('admin.donations.overview.fingerprint')} <code>{fingerprint}</code>.{' '}
            {t('admin.donations.overview.compareAddresses')}
          </p>
          <ol className="adm-mono">
            {addresses.map((a) => (
              <li key={a}>{a}</li>
            ))}
          </ol>
        </div>
      )}
      {saved && (
        <Notice variant="success" role="status">
          {saved}
        </Notice>
      )}
      <ReasonDialog
        open={confirm}
        title={t('admin.donations.overview.confirmTitle')}
        description={t('admin.donations.overview.confirmDescription')}
        confirmLabel={t('admin.donations.overview.confirmLabel')}
        onCancel={() => setConfirm(false)}
        onConfirm={async (reason) => {
          const r = await api.setWallet({
            descriptor,
            network,
            gap_limit: Number(gap) || 20,
            reason,
          });
          setSaved(
            t('admin.donations.overview.walletActive', {
              fingerprint: r.fingerprint,
              network: r.network,
            }),
          );
          setConfirm(false);
          setDescriptor('');
          setAddresses(null);
          onSaved();
        }}
      />
    </section>
  );
}

function MonthReport() {
  const t = useT();
  const res = useResource('don-report', () => api.getReport());
  return (
    <section className="ss-card" aria-labelledby="don-report-h">
      <h2 id="don-report-h">{t('admin.donations.overview.thisMonth')}</h2>
      <Async res={res} label={t('admin.donations.overview.loadingReport')}>
        {(r) => (
          <>
            <p>
              {t('admin.donations.overview.credited')} <strong>{usd(r.credited_cents)}</strong>
            </p>
            {r.by_purpose.length > 0 && (
              <ul className="ss-list">
                {r.by_purpose.map((p) => (
                  <li key={p.purpose} className="ss-row">
                    <span className="ss-grow">{p.purpose.replace('_', ' ')}</span>
                    <span>
                      {usd(p.cents)} ({p.count})
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </Async>
    </section>
  );
}

export function DonationsOverviewSection() {
  const t = useT();
  const res = useResource('don-status', api.getStatus);
  return (
    <>
      <SectionHeader
        title={t('admin.donations.overview.donations')}
        description={t('admin.donations.overview.description')}
        actions={
          <Button variant="secondary" onClick={() => void res.reload()}>
            {t('admin.donations.overview.refresh')}
          </Button>
        }
      />
      <Async res={res} label={t('admin.donations.overview.loadingDonations')}>
        {(s) => (
          <>
            <Health s={s} />
            <Rates s={s} />
          </>
        )}
      </Async>
      <WalletForm onSaved={() => void res.reload()} />
      <MonthReport />
    </>
  );
}
