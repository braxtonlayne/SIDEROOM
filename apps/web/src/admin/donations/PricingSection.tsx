// Admin > Donations > Pricing (DON-ADM-002, DON-ADM-005): prices, thresholds, per-level perks, invoice
// rules and the hosting cost; the cost calculator of doc 08 section 6.4; leaderboard moderation.
// Edits send only the changed keys as a merge patch, with a reason.
import { Button, Notice, TextInput } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { usd } from '../../support/api';
import { useT, type MessageKey } from '../../i18n';
import { Async, ReasonDialog, SectionHeader, useResource } from '../ui';
import * as api from './api';
import { breakEven, suggestedPrice, type CostInputs, type LevelShape } from './calculator';
import './donations.css';

type NumKey = {
  [K in keyof api.AdminConfig]: api.AdminConfig[K] extends number ? K : never;
}[keyof api.AdminConfig];

const FIELDS: { key: NumKey; label: MessageKey; hint?: MessageKey }[] = [
  { key: 'patron_price_cents', label: 'admin.donations.pricing.fieldPatronPrice' },
  { key: 'patron_max_months', label: 'admin.donations.pricing.fieldPatronMaxMonths' },
  { key: 'grace_days', label: 'admin.donations.pricing.fieldGraceDays' },
  { key: 'pool_window_days', label: 'admin.donations.pricing.fieldPoolWindow' },
  { key: 'invite_credit_price_cents', label: 'admin.donations.pricing.fieldInviteCreditPrice' },
  { key: 'min_donation_cents', label: 'admin.donations.pricing.fieldMinDonation' },
  { key: 'max_donation_cents', label: 'admin.donations.pricing.fieldMaxDonation' },
  {
    key: 'confirmations',
    label: 'admin.donations.pricing.fieldConfirmations',
    hint: 'admin.donations.pricing.fieldConfirmationsHint',
  },
  { key: 'finality_confirmations', label: 'admin.donations.pricing.fieldFinality' },
  { key: 'invoice_expiry_minutes', label: 'admin.donations.pricing.fieldExpiry' },
  {
    key: 'tolerance_bps',
    label: 'admin.donations.pricing.fieldTolerance',
    hint: 'admin.donations.pricing.fieldToleranceHint',
  },
  { key: 'late_watch_days', label: 'admin.donations.pricing.fieldLateWatch' },
  {
    key: 'hosting_cost_cents',
    label: 'admin.donations.pricing.fieldHosting',
    hint: 'admin.donations.pricing.fieldHostingHint',
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
  label: MessageKey;
}[] = [
  { key: 'upload_mib', label: 'admin.donations.pricing.perkUpload' },
  { key: 'video_senders', label: 'admin.donations.pricing.perkVideoSenders' },
  { key: 'stream_viewers', label: 'admin.donations.pricing.perkViewers' },
  { key: 'audio_kbps', label: 'admin.donations.pricing.perkAudio' },
  { key: 'watch_parties', label: 'admin.donations.pricing.perkWatchParties' },
  { key: 'downloads_per_day', label: 'admin.donations.pricing.perkDownloads' },
];

function ConfigForm({ cfg, onSaved }: { cfg: api.AdminConfig; onSaved: () => void }) {
  const t = useT();
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
      const next = [...d.level_thresholds_cents] as [number, number, number];
      next[i] = Number(v.replace(/\D/g, '')) || 0;
      return { ...d, level_thresholds_cents: next };
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
      <h2 id="don-prices-h">{t('admin.donations.pricing.pricesAndRules')}</h2>
      <label className="adm-check-row">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => setDraft((d) => ({ ...d, enabled: e.target.checked }))}
        />{' '}
        {t('admin.donations.pricing.donationsOn')}
      </label>
      <div className="adm-grid2">
        {FIELDS.map((f) => (
          <TextInput
            key={f.key}
            label={t(f.label)}
            {...(f.hint ? { hint: t(f.hint) } : {})}
            inputMode="numeric"
            value={String(draft[f.key])}
            onChange={(e) => setNum(f.key, e.target.value)}
          />
        ))}
        {[0, 1, 2].map((i) => (
          <TextInput
            key={i}
            label={t('admin.donations.pricing.levelTotal', { level: i + 1 })}
            inputMode="numeric"
            value={String(draft.level_thresholds_cents[i])}
            onChange={(e) => setThreshold(i, e.target.value)}
          />
        ))}
      </div>
      <h3>{t('admin.donations.pricing.perksPerTier')}</h3>
      <div className="adm-table-wrap">
        <table className="adm-table">
          <caption className="us-sr">{t('admin.donations.pricing.perksPerTier')}</caption>
          <thead>
            <tr>
              <th scope="col">{t('admin.donations.pricing.colTier')}</th>
              {PERK_FIELDS.map((p) => (
                <th key={p.key} scope="col">
                  {t(p.label)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(
              [
                ['admin.donations.pricing.tierFree', 'free'],
                ['admin.donations.pricing.tierPatron', 'patron'],
                ['admin.donations.pricing.tierLevel1', 0],
                ['admin.donations.pricing.tierLevel2', 1],
                ['admin.donations.pricing.tierLevel3', 2],
              ] as const
            ).map(([labelKey, tier]) => {
              const label = t(labelKey);
              const perks = typeof tier === 'number' ? draft.levels[tier] : draft[tier];
              return (
                <tr key={labelKey}>
                  <th scope="row">{label}</th>
                  {PERK_FIELDS.map((p) => (
                    <td key={p.key}>
                      <input
                        aria-label={t('admin.donations.pricing.perkAria', {
                          tier: label,
                          perk: t(p.label),
                        })}
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
      <p className="ss-meta">{t('admin.donations.pricing.perksNote')}</p>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <div className="adm-actions">
        <Button variant="secondary" disabled={!dirty} onClick={() => setDraft(cfg)}>
          {t('admin.donations.pricing.reset')}
        </Button>
        <Button disabled={!dirty} onClick={() => setSaving(true)}>
          {t('admin.donations.pricing.saveChanges')}
        </Button>
      </div>
      <ReasonDialog
        open={saving}
        title={t('admin.donations.pricing.saveTitle')}
        description={t('admin.donations.pricing.saveDescription', {
          count: Object.keys(changes).length,
        })}
        confirmLabel={t('admin.donations.pricing.save')}
        onCancel={() => setSaving(false)}
        onConfirm={async (reason) => {
          await api.patchConfig(changes as Partial<api.AdminConfig>, reason);
          setSaving(false);
          setNotice(t('admin.donations.pricing.saved'));
          onSaved();
        }}
      />
    </section>
  );
}

function Calculator({ cfg }: { cfg: api.AdminConfig }) {
  const t = useT();
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
      <h2 id="don-calc-h">{t('admin.donations.pricing.calcTitle')}</h2>
      <p className="ss-meta">{t('admin.donations.pricing.calcIntro')}</p>
      <div className="adm-grid2">
        <TextInput
          label={t('admin.donations.pricing.serverCost')}
          inputMode="decimal"
          value={serverCost}
          onChange={(e) => setServerCost(e.target.value)}
        />
        <TextInput
          label={t('admin.donations.pricing.uplink')}
          inputMode="numeric"
          value={uplink}
          onChange={(e) => setUplink(e.target.value)}
        />
        <TextInput
          label={t('admin.donations.pricing.heavyHours')}
          inputMode="numeric"
          value={hours}
          onChange={(e) => setHours(e.target.value)}
        />
        <TextInput
          label={t('admin.donations.pricing.floorPrice')}
          inputMode="decimal"
          value={floor}
          onChange={(e) => setFloor(e.target.value)}
        />
      </div>
      <table className="adm-table">
        <caption className="us-sr">{t('admin.donations.pricing.calcCaption')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('admin.donations.pricing.colLevel')}</th>
            <th scope="col">{t('admin.donations.pricing.colSuggested')}</th>
            <th scope="col">{t('admin.donations.pricing.colConfigured')}</th>
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
        {t('admin.donations.pricing.breakEven', {
          cost: dollars(inputs.serverCost),
          patrons: even.patrons,
          levels: even.levels
            .map((c, i) => t('admin.donations.pricing.levelServers', { count: c, level: i + 1 }))
            .join(t('admin.donations.pricing.orSeparator')),
        })}
      </p>
    </section>
  );
}

function Moderation() {
  const t = useT();
  const [id, setId] = useState('');
  const [what, setWhat] = useState<'user' | 'guild'>('user');
  const [hide, setHide] = useState(true);
  const [open, setOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <section className="ss-card" aria-labelledby="don-mod-h">
      <h2 id="don-mod-h">{t('admin.donations.pricing.modTitle')}</h2>
      <p className="ss-meta">{t('admin.donations.pricing.modIntro')}</p>
      <div className="ss-toolbar">
        <label>
          {t('admin.donations.pricing.who')}{' '}
          <select value={what} onChange={(e) => setWhat(e.target.value as typeof what)}>
            <option value="user">{t('admin.donations.pricing.whoAccount')}</option>
            <option value="guild">{t('admin.donations.pricing.whoServer')}</option>
          </select>
        </label>
        <TextInput
          label={t('admin.donations.pricing.id')}
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
          {t('admin.donations.pricing.hide')}
        </Button>
        <Button
          variant="secondary"
          disabled={!/^\d+$/.test(id.trim())}
          onClick={() => {
            setHide(false);
            setOpen(true);
          }}
        >
          {t('admin.donations.pricing.showAgain')}
        </Button>
      </div>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <ReasonDialog
        open={open}
        title={
          hide ? t('admin.donations.pricing.hideTitle') : t('admin.donations.pricing.showTitle')
        }
        confirmLabel={hide ? t('admin.donations.pricing.hide') : t('admin.donations.pricing.show')}
        onCancel={() => setOpen(false)}
        onConfirm={async (reason) => {
          await api.hideFromBoards({
            ...(what === 'user' ? { user_id: id.trim() } : { guild_id: id.trim() }),
            hidden: hide,
            reason,
          });
          setNotice(
            hide ? t('admin.donations.pricing.hidden') : t('admin.donations.pricing.visibleAgain'),
          );
          setOpen(false);
        }}
      />
    </section>
  );
}

export function PricingSection() {
  const t = useT();
  const res = useResource('don-config', api.getConfig);
  return (
    <>
      <SectionHeader
        title={t('admin.donations.pricing.title')}
        description={t('admin.donations.pricing.description')}
      />
      <Async res={res} label={t('admin.donations.pricing.loading')}>
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
