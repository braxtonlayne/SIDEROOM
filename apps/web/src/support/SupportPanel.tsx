// Settings > Support / Patron (DON-UI-001..004): Patron status and months, one-off donations, invite
// credits, supporting a server, leaderboard privacy and the invoice history. Everything that costs money
// goes through an invoice dialog; nothing here signs or sends a payment.
import { Badge, Button, Notice, ProgressBar, Switch, useToast } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useCallback, useEffect, useState } from 'react';
import { Card, Field, Loaded, SectionTitle } from '../settings/user/parts';
import { useResource } from '../settings/user/api';
import { useGuilds } from '../store';
import * as api from './api';
import { errorText, usd, type DonationConfig, type Invoice, type PerkSet } from './api';
import { InvoiceModal } from './InvoiceModal';
import './support.css';

const MONTH_CHOICES = [1, 3, 6, 12];
const PRESETS = [500, 1000, 2500, 5000];

export function perkLines(p: PerkSet): string[] {
  const lines = [`Uploads up to ${p.upload_mib} MB per file`];
  const best = [...p.video_profiles].sort((a, b) => b.height * b.fps - a.height * a.fps)[0];
  if (best) lines.push(`Video up to ${best.height}p${best.fps}`);
  if (p.video_senders > 0) lines.push(`${p.video_senders} video senders per voice channel`);
  if (p.stream_viewers > 0) lines.push(`${p.stream_viewers} viewers per stream`);
  if (p.audio_kbps > 0) lines.push(`${p.audio_kbps} kbps voice audio`);
  if (p.watch_parties > 0) lines.push(`${p.watch_parties} concurrent watch parties`);
  if (p.downloads_per_day > 0)
    lines.push(`${p.downloads_per_day} Cinema and Library downloads a day`);
  if (p.exempt_from_sender_limit) lines.push('Your video never counts against a server limit');
  return lines;
}

/** Money text for a custom amount typed in dollars; `null` when it is not a usable amount. */
export function parseDollars(raw: string): number | null {
  const t = raw.trim().replace(/^\$/, '');
  if (!/^\d{1,7}(\.\d{1,2})?$/.test(t)) return null;
  const cents = Math.round(Number(t) * 100);
  return cents > 0 ? cents : null;
}

function HostingBar({ cfg }: { cfg: DonationConfig }) {
  const { cost_cents, donated_this_month_cents } = cfg.hosting;
  return (
    <Card
      id="sup-hosting"
      title="Keeping the lights on"
      description="Almost everything here is free. Donations pay for hosting and unlock the few things that cost real money: big uploads, lots of video, and downloads."
    >
      {cost_cents > 0 ? (
        <>
          <ProgressBar
            value={Math.min(donated_this_month_cents, cost_cents)}
            max={cost_cents}
            label="Donations this month against the monthly hosting cost"
          />
          <p className="us-muted">
            {usd(donated_this_month_cents)} of {usd(cost_cents)} this month
          </p>
        </>
      ) : (
        <p className="us-muted">{usd(donated_this_month_cents)} donated this month.</p>
      )}
    </Card>
  );
}

function PatronCard({
  cfg,
  me,
  onPay,
  disabled,
  busy,
}: {
  cfg: DonationConfig;
  me: api.Me | undefined;
  onPay: (months: number) => void;
  disabled: boolean;
  busy: boolean;
}) {
  const p = me?.patron;
  let status = 'You are not a Patron.';
  if (p?.permanent) status = 'You are a Patron, with no end date.';
  else if (p?.active && p.until_ms)
    status = `You are a Patron until ${new Date(p.until_ms).toLocaleDateString()}.`;
  else if (p?.in_grace && p.until_ms)
    status = `Your Patron time ended ${new Date(p.until_ms).toLocaleDateString()}. Perks continue for ${cfg.patron.grace_days} days.`;
  return (
    <Card
      id="sup-patron"
      title="Patron"
      description={`${usd(cfg.patron.price_cents)} a month. Perks follow you into every server, DM and call.`}
      actions={p?.active || p?.in_grace ? <Badge variant="success">Active</Badge> : undefined}
    >
      <p role="status">{status}</p>
      <ul className="sup-perks">
        {perkLines(cfg.patron.perks).map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
      <div className="sup-choices" role="group" aria-label="Patron months">
        {MONTH_CHOICES.filter((m) => m <= cfg.patron.max_months).map((m) => (
          <Button
            key={m}
            variant={m === 1 ? 'primary' : 'secondary'}
            disabled={disabled || busy}
            onClick={() => onPay(m)}
          >
            {m} month{m === 1 ? '' : 's'}, {usd(m * cfg.patron.price_cents)}
          </Button>
        ))}
      </div>
      <p className="us-muted">
        Paying again adds time after what you have. You can prepay up to {cfg.patron.max_months}{' '}
        months.
      </p>
    </Card>
  );
}

export function AmountPicker({
  cfg,
  onPick,
  cta,
  disabled,
}: {
  cfg: DonationConfig;
  onPick: (cents: number) => void;
  cta: string;
  disabled: boolean;
}) {
  const [custom, setCustom] = useState('');
  const cents = parseDollars(custom);
  const bad =
    custom.trim() !== '' &&
    (cents === null || cents < cfg.min_donation_cents || cents > cfg.max_donation_cents);
  return (
    <div className="sup-amount">
      <div className="sup-choices" role="group" aria-label="Amount">
        {PRESETS.filter((c) => c >= cfg.min_donation_cents).map((c) => (
          <Button
            key={c}
            variant="secondary"
            disabled={disabled}
            onClick={() => onPick(c)}
            aria-label={`${cta} ${usd(c)}`}
          >
            {usd(c)}
          </Button>
        ))}
      </div>
      <form
        className="sup-custom"
        onSubmit={(e) => {
          e.preventDefault();
          if (cents && !bad) onPick(cents);
        }}
      >
        <label htmlFor="sup-custom-amount">Other amount (USD)</label>
        <input
          id="sup-custom-amount"
          inputMode="decimal"
          value={custom}
          onChange={(e) => setCustom(e.target.value)}
          aria-invalid={bad}
          aria-describedby={bad ? 'sup-custom-err' : undefined}
          placeholder="7.50"
        />
        <Button type="submit" disabled={disabled || !cents || bad}>
          {cta}
        </Button>
        {bad && (
          <span id="sup-custom-err" className="sup-error" role="alert">
            Use an amount from {usd(cfg.min_donation_cents)} to {usd(cfg.max_donation_cents)}.
          </span>
        )}
      </form>
    </div>
  );
}

function ServerCard({
  cfg,
  onPay,
  disabled,
}: {
  cfg: DonationConfig;
  onPay: (guildId: string, cents: number, anonymous: boolean) => void;
  disabled: boolean;
}) {
  const guilds = useGuilds().filter((g) => g.id !== '@me' && !g.unavailable);
  const [guildId, setGuildId] = useState('');
  const [anon, setAnon] = useState(false);
  const chosen = guildId || guilds[0]?.id || '';
  return (
    <Card
      id="sup-server"
      title="Support a server"
      description="Members pool money into a server's monthly total. When it passes a level, everyone in that server gets more upload size, video senders and voice quality."
    >
      <table className="sup-levels">
        <caption className="us-sr">Server levels</caption>
        <thead>
          <tr>
            <th scope="col">Level</th>
            <th scope="col">Monthly total</th>
            <th scope="col">What members get</th>
          </tr>
        </thead>
        <tbody>
          {cfg.levels.map((l) => (
            <tr key={l.level}>
              <th scope="row">{l.level}</th>
              <td>{usd(l.threshold_cents)}</td>
              <td>{perkLines(l.perks).join('. ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {guilds.length === 0 ? (
        <p className="us-muted">Join a server to support it.</p>
      ) : (
        <>
          <div className="sup-custom">
            <label htmlFor="sup-guild">Server</label>
            <select id="sup-guild" value={chosen} onChange={(e) => setGuildId(e.target.value)}>
              {guilds.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </div>
          <Switch
            label="Contribute anonymously"
            description="Your name is not shown among the server's top contributors."
            checked={anon}
            onChange={(e) => setAnon(e.target.checked)}
          />
          <AmountPicker
            cfg={cfg}
            cta="Support"
            disabled={disabled || !chosen}
            onPick={(c) => onPay(chosen, c, anon)}
          />
        </>
      )}
    </Card>
  );
}

function PrefsCard({ me, onChange }: { me: api.Me; onChange: (p: api.Prefs) => void }) {
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);
  const save = async (patch: Partial<api.Prefs>) => {
    setError(null);
    try {
      onChange(await api.patchPrefs(patch));
      toast.show({ title: 'Saved', duration: 1500 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  const p = me.preferences;
  return (
    <Card
      id="sup-privacy"
      title="Leaderboards and badge"
      description="Donors are listed on the public leaderboards unless you hide your name, your amount, or both."
      actions={<Link to="/support/leaderboards">View leaderboards</Link>}
    >
      <Switch
        label="Hide my name (shown as Anonymous patron)"
        checked={p.hide_name}
        onChange={(e) => void save({ hide_name: e.target.checked })}
      />
      <Switch
        label="Hide my amount (rank only)"
        checked={p.hide_amount}
        onChange={(e) => void save({ hide_amount: e.target.checked })}
      />
      <Switch
        label="Hide my Patron badge"
        checked={p.hide_badge}
        onChange={(e) => void save({ hide_badge: e.target.checked })}
      />
      {error && (
        <Notice variant="danger" role="alert">
          {error}
        </Notice>
      )}
    </Card>
  );
}

function History({ onOpen, version }: { onOpen: (inv: Invoice) => void; version: number }) {
  const res = useResource(() => api.listInvoices({ limit: 10 }));
  const reload = res.reload;
  useEffect(() => {
    if (version > 0) reload();
  }, [version, reload]);
  const [more, setMore] = useState<Invoice[]>([]);
  const [next, setNext] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const loadMore = async () => {
    const cursor = next === undefined ? res.data?.next_before : next;
    if (!cursor) return;
    try {
      const page = await api.listInvoices({ before: cursor, limit: 10 });
      setMore((m) => [...m, ...page.items]);
      setNext(page.next_before);
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <Card id="sup-history" title="History" description="Your invoices and receipts.">
      <Loaded res={res} what="invoices">
        {(page) => {
          const rows = [...page.items, ...more];
          const cursor = next === undefined ? page.next_before : next;
          return rows.length === 0 ? (
            <p className="us-muted">No invoices yet.</p>
          ) : (
            <>
              <ul className="us-list">
                {rows.map((inv) => (
                  <li key={inv.id}>
                    <div className="us-row-main">
                      <strong>
                        {api.PURPOSE_COPY[inv.purpose]}, {usd(inv.usd_cents)}
                      </strong>
                      <p className="us-muted">
                        {new Date(inv.created_at_ms).toLocaleDateString()} ·{' '}
                        {api.STATE_COPY[inv.state]}
                      </p>
                    </div>
                    <Button variant="secondary" size="sm" onClick={() => onOpen(inv)}>
                      {api.isPaid(inv.state) ? 'Receipt' : 'Open'}
                      <span className="us-sr"> invoice {inv.id}</span>
                    </Button>
                  </li>
                ))}
              </ul>
              {cursor && (
                <Button variant="secondary" size="sm" onClick={() => void loadMore()}>
                  Show more
                </Button>
              )}
              {error && (
                <Notice variant="danger" role="alert">
                  {error}
                </Notice>
              )}
            </>
          );
        }}
      </Loaded>
    </Card>
  );
}

/** The section body; also used by tests and the standalone page. */
export function SupportSection() {
  const cfg = useResource(api.getConfig);
  const me = useResource(api.getMe);
  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const [credits, setCredits] = useState(1);

  const create = useCallback(async (body: api.CreateInvoice) => {
    setBusy(true);
    setError(null);
    try {
      setInvoice(await api.createInvoice(body));
      setVersion((v) => v + 1);
    } catch (e) {
      setError(errorText(e, 'Could not create the invoice.'));
    } finally {
      setBusy(false);
    }
  }, []);
  const changed = useCallback(() => {
    setVersion((v) => v + 1);
    me.reload();
    cfg.reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <>
      <SectionTitle>Support / Patron</SectionTitle>
      <Loaded res={cfg} what="donation settings">
        {(c) => (
          <>
            {!c.enabled && (
              <Notice variant="info" title="Donations are not switched on">
                This installation has no payment wallet configured yet, so invoices cannot be
                created. Ask the operator.
              </Notice>
            )}
            {c.enabled && c.rate === null && (
              <Notice variant="warning" title="No price available">
                The Bitcoin price could not be read right now, so new invoices will fail until it
                can. Try again in a few minutes.
              </Notice>
            )}
            {error && (
              <Notice variant="danger" role="alert" title="That did not work">
                {error}
              </Notice>
            )}
            <HostingBar cfg={c} />
            <PatronCard
              cfg={c}
              me={me.data}
              disabled={!c.enabled}
              busy={busy}
              onPay={(months) => void create({ purpose: 'patron', months })}
            />
            <Card
              id="sup-donate"
              title="Donate"
              description="A one-off donation to the community. It buys no perks, and is greatly appreciated."
            >
              <AmountPicker
                cfg={c}
                cta="Donate"
                disabled={!c.enabled || busy}
                onPick={(usd_cents) => void create({ purpose: 'donate', usd_cents })}
              />
            </Card>
            <ServerCard
              cfg={c}
              disabled={!c.enabled || busy}
              onPay={(guild_id, usd_cents, anonymous) =>
                void create({ purpose: 'server_level', guild_id, usd_cents, anonymous })
              }
            />
            <Card
              id="sup-credits"
              title="Invite credits"
              description={`Extra platform invites cost ${usd(c.invite_credit_price_cents)} each.`}
            >
              <div className="sup-custom">
                <label htmlFor="sup-credit-n">Credits</label>
                <select
                  id="sup-credit-n"
                  value={credits}
                  onChange={(e) => setCredits(Number(e.target.value))}
                >
                  {Array.from({ length: Math.min(c.max_invite_credits, 5) }, (_, i) => i + 1).map(
                    (n) => (
                      <option key={n} value={n}>
                        {n}
                      </option>
                    ),
                  )}
                </select>
                <Button
                  disabled={!c.enabled || busy}
                  onClick={() => void create({ purpose: 'invite_credit', credits })}
                >
                  Buy for {usd(credits * c.invite_credit_price_cents)}
                </Button>
              </div>
            </Card>
            {me.data && (
              <Card id="sup-limits" title="What you have">
                <dl>
                  <Field label="Upload size">
                    {me.data.limits.upload_bytes / 1048576} MB per file
                  </Field>
                  <Field label="Downloads">
                    {me.data.limits.downloads_per_day > 0
                      ? `${me.data.limits.downloads_per_day} a day`
                      : 'Patrons only'}
                  </Field>
                </dl>
              </Card>
            )}
            {me.data && (
              <PrefsCard
                me={me.data}
                onChange={(preferences) => me.setData((m) => (m ? { ...m, preferences } : m))}
              />
            )}
          </>
        )}
      </Loaded>
      <History version={version} onOpen={setInvoice} />
      {invoice && (
        <InvoiceModal
          invoice={invoice}
          onClose={() => {
            setInvoice(null);
            changed();
          }}
          onChanged={changed}
        />
      )}
    </>
  );
}
