// Admin > Donations > Pricing (DON-ADM-002, DON-ADM-005): prices, thresholds, per-level perks, invoice
// rules and the hosting cost; the cost calculator of doc 08 section 6.4; leaderboard moderation.
// Edits send only the changed keys as a merge patch, with a reason.
import { Button, Notice, TextInput } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { usd } from '../../support/api';
import { Async, ReasonDialog, SectionHeader, useResource } from '../ui';
import * as api from './api';
import { breakEven, suggestedPrice, type CostInputs, type LevelShape } from './calculator';
import './donations.css';

type NumKey = {
  [K in keyof api.AdminConfig]: api.AdminConfig[K] extends number ? K : never;
}[keyof api.AdminConfig];

const FIELDS: { key: NumKey; label: string; hint?: string }[] = [
  { key: 'patron_price_cents', label: 'Patron price per month (USD cents)' },
  { key: 'patron_max_months', label: 'Most months in one invoice' },
  { key: 'grace_days', label: 'Grace days after Patron or a level lapses' },
  { key: 'pool_window_days', label: 'Server pool window (days)' },
  { key: 'invite_credit_price_cents', label: 'Invite credit price (USD cents)' },
  { key: 'min_donation_cents', label: 'Smallest donation (USD cents)' },
  { key: 'max_donation_cents', label: 'Largest donation (USD cents)' },
  {
    key: 'confirmations',
    label: 'Confirmations before crediting',
    hint: '0 credits from the mempool',
  },
  { key: 'finality_confirmations', label: 'Keep watching until this many confirmations' },
  { key: 'invoice_expiry_minutes', label: 'Invoice expiry (minutes)' },
  { key: 'tolerance_bps', label: 'Underpay/overpay tolerance (basis points)', hint: '100 = 1%' },
  { key: 'late_watch_days', label: 'Watch expired invoices for (days)' },
  {
    key: 'hosting_cost_cents',
    label: 'Monthly hosting cost (USD cents)',
    hint: 'For the progress bar',
  },
];

const PERK_FIELDS: {
  key:
    | 'upload_mib'
    | 'video_senders'
    | 'stream_viewers'
    | 'audio_kbps'
    | 'watch_parties'
    | 'downloads_per_day';
  label: string;
}[] = [
  { key: 'upload_mib', label: 'Upload MB' },
  { key: 'video_senders', label: 'Video senders' },
  { key: 'stream_viewers', label: 'Viewers' },
  { key: 'audio_kbps', label: 'Audio kbps' },
  { key: 'watch_parties', label: 'Watch parties' },
  { key: 'downloads_per_day', label: 'Downloads/day' },
];

function ConfigForm({ cfg, onSaved }: { cfg: api.AdminConfig; onSaved: () => void }) {
  const [draft, setDraft] = useState<api.AdminConfig>(cfg);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const changes = useMemo(() => {
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(draft) as (keyof api.AdminConfig)[]) {
      if (JSON.stringify(draft[k]) !== JSON.stringify(cfg[k])) out[k] = draft[k];
    }
    return out;
  }, [draft, cfg]);
  const dirty = Object.keys(changes).length > 0;

  const setNum = (key: NumKey, v: string) =>
    setDraft((d) => ({ ...d, [key]: Number(v.replace(/\D/g, '')) || 0 }));
  const setThreshold = (i: number, v: string) =>
    setDraft((d) => {
      const t = [...d.level_thresholds_cents] as [number, number, number];
      t[i] = Number(v.replace(/\D/g, '')) || 0;
      return { ...d, level_thresholds_cents: t };
    });
  const setPerk = (
    tier: 'patron' | 'free' | number,
    key: (typeof PERK_FIELDS)[number]['key'],
    v: string,
  ) =>
    setDraft((d) => {
      const n = Number(v.replace(/\D/g, '')) || 0;
      if (typeof tier === 'number') {
        const levels = d.levels.map((l, i) =>
          i === tier ? { ...l, [key]: n } : l,
        ) as typeof d.levels;
        return { ...d, levels };
      }
      return { ...d, [tier]: { ...d[tier], [key]: n } };
    });

  return (
    <section className="ss-card" aria-labelledby="don-prices-h">
      <h2 id="don-prices-h">Prices and rules</h2>
      <label className="adm-check-row">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => setDraft((d) => ({ ...d, enabled: e.target.checked }))}
        />{' '}
        Donations are on
      </label>
      <div className="adm-grid2">
        {FIELDS.map((f) => (
          <TextInput
            key={f.key}
            label={f.label}
            {...(f.hint ? { hint: f.hint } : {})}
            inputMode="numeric"
            value={String(draft[f.key])}
            onChange={(e) => setNum(f.key, e.target.value)}
          />
        ))}
        {[0, 1, 2].map((i) => (
          <TextInput
            key={i}
            label={`Level ${i + 1} monthly total (USD cents)`}
            inputMode="numeric"
            value={String(draft.level_thresholds_cents[i])}
            onChange={(e) => setThreshold(i, e.target.value)}
          />
        ))}
      </div>
      <h3>Perks per tier</h3>
      <div className="adm-table-wrap">
        <table className="adm-table">
          <caption className="us-sr">Perks per tier</caption>
          <thead>
            <tr>
              <th scope="col">Tier</th>
              {PERK_FIELDS.map((p) => (
                <th key={p.key} scope="col">
                  {p.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(
              [
                ['Free', 'free'],
                ['Patron', 'patron'],
                ['Level 1', 0],
                ['Level 2', 1],
                ['Level 3', 2],
              ] as const
            ).map(([label, tier]) => {
              const perks = typeof tier === 'number' ? draft.levels[tier] : draft[tier];
              return (
                <tr key={label}>
                  <th scope="row">{label}</th>
                  {PERK_FIELDS.map((p) => (
                    <td key={p.key}>
                      <input
                        aria-label={`${label} ${p.label}`}
                        inputMode="numeric"
                        size={6}
                        value={String(perks[p.key])}
                        onChange={(e) => setPerk(tier, p.key, e.target.value)}
                      />
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="ss-meta">
        A tier that leaves a perk at 0 never lowers what another tier gives. Server levels never
        unlock downloads.
      </p>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <div className="adm-actions">
        <Button variant="secondary" disabled={!dirty} onClick={() => setDraft(cfg)}>
          Reset
        </Button>
        <Button disabled={!dirty} onClick={() => setSaving(true)}>
          Save changes
        </Button>
      </div>
      <ReasonDialog
        open={saving}
        title="Save donation settings"
        description={`${Object.keys(changes).length} setting(s) change. They take effect at once.`}
        confirmLabel="Save"
        onCancel={() => setSaving(false)}
        onConfirm={async (reason) => {
          await api.patchConfig(changes as Partial<api.AdminConfig>, reason);
          setSaving(false);
          setNotice('Saved.');
          onSaved();
        }}
      />
    </section>
  );
}

function Calculator({ cfg }: { cfg: api.AdminConfig }) {
  const [serverCost, setServerCost] = useState('128');
  const [uplink, setUplink] = useState('1000');
  const [hours, setHours] = useState('40');
  const [floor, setFloor] = useState('1');
  const n = (s: string) => Math.max(0, Number(s) || 0);
  const inputs: CostInputs = {
    serverCost: n(serverCost),
    uplinkMbps: n(uplink),
    utilisationHours: n(hours),
    floorPrice: n(floor),
  };
  const bitrate = [4, 8, 8];
  const rows = cfg.levels.map((l, i) => {
    const shape: LevelShape = {
      level: i + 1,
      viewersCap: l.stream_viewers,
      sendersCap: l.video_senders,
      maxBitrateMbps: bitrate[i] ?? 8,
    };
    return {
      level: i + 1,
      suggested: suggestedPrice(shape, inputs),
      configured: cfg.level_thresholds_cents[i]! / 100,
    };
  });
  const even = breakEven(
    inputs.serverCost,
    cfg.patron_price_cents / 100,
    cfg.level_thresholds_cents.map((c) => c / 100),
  );
  const dollars = (v: number) => usd(Math.round(v * 100));
  return (
    <section className="ss-card" aria-labelledby="don-calc-h">
      <h2 id="don-calc-h">Cost calculator</h2>
      <p className="ss-meta">
        Enter what the server really costs. Suggested prices cover the expected video egress with a
        safety factor of three (docs/08 section 6.4).
      </p>
      <div className="adm-grid2">
        <TextInput
          label="Server cost per month (USD)"
          inputMode="decimal"
          value={serverCost}
          onChange={(e) => setServerCost(e.target.value)}
        />
        <TextInput
          label="Uplink (Mbps)"
          inputMode="numeric"
          value={uplink}
          onChange={(e) => setUplink(e.target.value)}
        />
        <TextInput
          label="Heavy-use hours per month"
          inputMode="numeric"
          value={hours}
          onChange={(e) => setHours(e.target.value)}
        />
        <TextInput
          label="Lowest price worth charging (USD)"
          inputMode="decimal"
          value={floor}
          onChange={(e) => setFloor(e.target.value)}
        />
      </div>
      <table className="adm-table">
        <caption className="us-sr">Suggested against configured prices</caption>
        <thead>
          <tr>
            <th scope="col">Level</th>
            <th scope="col">Suggested</th>
            <th scope="col">Configured</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.level}>
              <th scope="row">{r.level}</th>
              <td>{dollars(r.suggested)}</td>
              <td>{dollars(r.configured)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p data-testid="break-even">
        To cover {dollars(inputs.serverCost)} a month you need {even.patrons} Patrons, or{' '}
        {even.levels.map((c, i) => `${c} level-${i + 1} servers`).join(', or ')}.
      </p>
    </section>
  );
}

function Moderation() {
  const [id, setId] = useState('');
  const [what, setWhat] = useState<'user' | 'guild'>('user');
  const [hide, setHide] = useState(true);
  const [open, setOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <section className="ss-card" aria-labelledby="don-mod-h">
      <h2 id="don-mod-h">Leaderboard moderation</h2>
      <p className="ss-meta">
        Keep a donor or a server off the public boards, for example for an offensive name.
      </p>
      <div className="ss-toolbar">
        <label>
          Who{' '}
          <select value={what} onChange={(e) => setWhat(e.target.value as typeof what)}>
            <option value="user">Account</option>
            <option value="guild">Server</option>
          </select>
        </label>
        <TextInput
          label="Id"
          inputMode="numeric"
          value={id}
          onChange={(e) => setId(e.target.value)}
        />
        <Button
          variant="secondary"
          disabled={!/^\d+$/.test(id.trim())}
          onClick={() => {
            setHide(true);
            setOpen(true);
          }}
        >
          Hide
        </Button>
        <Button
          variant="secondary"
          disabled={!/^\d+$/.test(id.trim())}
          onClick={() => {
            setHide(false);
            setOpen(true);
          }}
        >
          Show again
        </Button>
      </div>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <ReasonDialog
        open={open}
        title={hide ? 'Hide from the leaderboards' : 'Show on the leaderboards again'}
        confirmLabel={hide ? 'Hide' : 'Show'}
        onCancel={() => setOpen(false)}
        onConfirm={async (reason) => {
          await api.hideFromBoards({
            ...(what === 'user' ? { user_id: id.trim() } : { guild_id: id.trim() }),
            hidden: hide,
            reason,
          });
          setNotice(hide ? 'Hidden from the boards.' : 'Visible on the boards again.');
          setOpen(false);
        }}
      />
    </section>
  );
}

export function PricingSection() {
  const res = useResource('don-config', api.getConfig);
  return (
    <>
      <SectionHeader
        title="Pricing and perks"
        description="Everything here is operator-configurable and applies without a restart."
      />
      <Async res={res} label="Loading settings">
        {(cfg) => (
          <>
            <ConfigForm key={JSON.stringify(cfg)} cfg={cfg} onSaved={() => void res.reload()} />
            <Calculator cfg={cfg} />
          </>
        )}
      </Async>
      <Moderation />
    </>
  );
}
