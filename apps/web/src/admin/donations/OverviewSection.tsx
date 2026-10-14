// Admin > Donations (DON-ADM-001): wallet configuration and health. The page never shows the descriptor
// back: only its short fingerprint. Pasting a seed or a private key is refused by the server.
import { Badge, Button, Notice, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { usd } from '../../support/api';
import { Async, ReasonDialog, SectionHeader, Stat, fmtMs, useResource } from '../ui';
import * as api from './api';
import './donations.css';

function ageText(ms: number | null) {
  if (!ms) return 'never';
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 90) return `${s} s ago`;
  if (s < 5400) return `${Math.round(s / 60)} min ago`;
  return `${Math.round(s / 3600)} h ago`;
}

function Health({ s }: { s: api.Status }) {
  const w = s.watcher;
  const healthy = !!w.last_ok_ms && !w.last_error;
  return (
    <section className="ss-card" aria-labelledby="don-health-h">
      <h2 id="don-health-h">Health</h2>
      <div className="adm-stats">
        <Stat label="Donations" value={s.enabled ? 'On' : 'Off'} />
        <Stat
          label="Watcher"
          value={healthy ? 'Healthy' : w.last_run_ms ? 'Problem' : 'Not run'}
          hint={`last ok ${ageText(w.last_ok_ms)}`}
        />
        <Stat label="Chain tip" value={w.tip_height ?? 'unknown'} />
        <Stat label="Needs attention" value={s.needs_attention} hint="underpaid, overpaid, late" />
      </div>
      {w.last_error && (
        <Notice variant="warning" title="The watcher reported a problem" role="status">
          {w.last_error}
        </Notice>
      )}
      {!s.enabled && (
        <Notice variant="info" title="Donations are off">
          They switch on once a wallet is set below, an Esplora endpoint is configured
          (SR_PAYMENTS_ESPLORA_URL) and the switch in Pricing is on.
        </Notice>
      )}
      {s.wallet ? (
        <dl className="adm-dl">
          <dt>Wallet</dt>
          <dd>
            <code>{s.wallet.fingerprint}</code> on {s.wallet.network}
          </dd>
          <dt>Next address index</dt>
          <dd>{s.wallet.next_index}</dd>
          <dt>Unused in a row</dt>
          <dd>
            {s.wallet.gap_used} of {s.wallet.gap_limit}{' '}
            {s.wallet.gap_used >= s.wallet.gap_limit && (
              <Badge variant="danger">gap limit reached</Badge>
            )}
          </dd>
        </dl>
      ) : (
        <p className="ss-meta">No wallet yet.</p>
      )}
      <h3>Invoices (last 90 days)</h3>
      <p className="ss-meta">
        {Object.entries(s.counts)
          .map(([k, n]) => `${k.replace('_', ' ')}: ${n}`)
          .join(' · ') || 'None yet'}
      </p>
    </section>
  );
}

function Rates({ s }: { s: api.Status }) {
  return (
    <section className="ss-card" aria-labelledby="don-rates-h">
      <h2 id="don-rates-h">Price sources</h2>
      {s.rate ? (
        <p>
          Current: <strong>{usd(s.rate.cents_per_btc)}</strong> per BTC ({s.rate.source})
          {s.rate.stale && <Badge variant="warning">stale</Badge>}
        </p>
      ) : (
        <Notice variant="warning" title="No price" role="status">
          No source answered yet. Invoices cannot be created without a price. Configure sources with
          SR_PAYMENTS_RATE_SOURCES.
        </Notice>
      )}
      {s.rate_sources.length > 0 && (
        <table className="adm-table">
          <caption className="us-sr">Price sources</caption>
          <thead>
            <tr>
              <th scope="col">Source</th>
              <th scope="col">Last price</th>
              <th scope="col">Last ok</th>
              <th scope="col">Error</th>
            </tr>
          </thead>
          <tbody>
            {s.rate_sources.map((r) => (
              <tr key={r.name}>
                <th scope="row">{r.name}</th>
                <td>{r.last_cents ? usd(r.last_cents) : ''}</td>
                <td>{r.last_ok_ms ? fmtMs(r.last_ok_ms) : 'never'}</td>
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
      <h2 id="don-wallet-h">Wallet</h2>
      <Notice variant="info" title="Watch-only">
        Paste a watch-only descriptor such as <code>wpkh(xpub.../0/*)</code>. The server derives one
        address per invoice and never holds a key, a seed, or any way to spend. A descriptor with a
        private key is refused.
      </Notice>
      <form className="adm-form" onSubmit={(e) => e.preventDefault()}>
        <TextArea
          label="Output descriptor (watch-only)"
          value={descriptor}
          rows={3}
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => setDescriptor(e.target.value)}
        />
        <label>
          Network{' '}
          <select value={network} onChange={(e) => setNetwork(e.target.value)}>
            <option value="mainnet">mainnet</option>
            <option value="testnet">testnet</option>
            <option value="signet">signet</option>
            <option value="regtest">regtest</option>
          </select>
        </label>
        <TextInput
          label="Gap limit"
          hint="How many unused addresses in a row before new invoices wait."
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
            Test derivation
          </Button>
          <Button disabled={!addresses} onClick={() => setConfirm(true)}>
            Use this wallet
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
            Fingerprint <code>{fingerprint}</code>. Compare these with the first receive addresses
            in your own wallet before using it:
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
        title="Use this wallet for new invoices"
        description="Open invoices of the previous wallet keep being watched."
        confirmLabel="Use wallet"
        onCancel={() => setConfirm(false)}
        onConfirm={async (reason) => {
          const r = await api.setWallet({
            descriptor,
            network,
            gap_limit: Number(gap) || 20,
            reason,
          });
          setSaved(`Wallet ${r.fingerprint} is active on ${r.network}.`);
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
  const res = useResource('don-report', () => api.getReport());
  return (
    <section className="ss-card" aria-labelledby="don-report-h">
      <h2 id="don-report-h">This month</h2>
      <Async res={res} label="Loading the report">
        {(r) => (
          <>
            <p>
              Credited: <strong>{usd(r.credited_cents)}</strong>
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
  const res = useResource('don-status', api.getStatus);
  return (
    <>
      <SectionHeader
        title="Donations"
        description="Self-custodial, watch-only Bitcoin. Payments go straight to your wallet; this server only watches."
        actions={
          <Button variant="secondary" onClick={() => void res.reload()}>
            Refresh
          </Button>
        }
      />
      <Async res={res} label="Loading donations">
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
