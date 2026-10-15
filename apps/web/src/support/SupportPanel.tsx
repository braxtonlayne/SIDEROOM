// Settings > Support / Patron (DON-UI-001..004): Patron status and months, one-off donations, invite
// credits, supporting a server, leaderboard privacy and the invoice history. Everything that costs money
// goes through an invoice dialog; nothing here signs or sends a payment.
import { Badge, Button, Notice, ProgressBar, Switch, useToast } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useCallback, useEffect, useState } from 'react';
import { Card, Field, Loaded, SectionTitle } from '../settings/user/parts';
import { useResource } from '../settings/user/api';
import { formatDate, t, useT } from '../i18n';
import { useGuilds } from '../store';
import * as api from './api';
import { errorText, usd, type DonationConfig, type Invoice, type PerkSet } from './api';
import { InvoiceModal } from './InvoiceModal';
import './support.css';

const MONTH_CHOICES = [1, 3, 6, 12];
const PRESETS = [500, 1000, 2500, 5000];

export function perkLines(p: PerkSet): string[] {
  const lines = [t('support.panel.perkUpload', { mib: p.upload_mib })];
  const best = [...p.video_profiles].sort((a, b) => b.height * b.fps - a.height * a.fps)[0];
  if (best) lines.push(t('support.panel.perkVideo', { height: best.height, fps: best.fps }));
  if (p.video_senders > 0) lines.push(t('support.panel.perkSenders', { n: p.video_senders }));
  if (p.stream_viewers > 0) lines.push(t('support.panel.perkViewers', { n: p.stream_viewers }));
  if (p.audio_kbps > 0) lines.push(t('support.panel.perkAudio', { n: p.audio_kbps }));
  if (p.watch_parties > 0) lines.push(t('support.panel.perkParties', { n: p.watch_parties }));
  if (p.downloads_per_day > 0)
    lines.push(t('support.panel.perkDownloads', { n: p.downloads_per_day }));
  if (p.exempt_from_sender_limit) lines.push(t('support.panel.perkExempt'));
  return lines;
}

/** One row of the free-versus-Patron table: a label and the text for each plan. */
export function perkRows(
  free: PerkSet,
  patron: PerkSet,
): { label: string; free: string; patron: string }[] {
  const best = (p: PerkSet) =>
    [...p.video_profiles].sort((a, b) => b.height * b.fps - a.height * a.fps)[0];
  const video = (p: PerkSet) => {
    const b = best(p);
    return b ? `${b.height}p${b.fps}` : '';
  };
  const num = (n: number, text: (n: number) => string) => (n > 0 ? text(n) : '');
  const mb = (n: number) => t('support.panel.tableMb', { n });
  const kbps = (n: number) => t('support.panel.tableKbps', { n });
  const perDay = (n: number) => t('support.panel.tablePerDay', { n });
  const rows = [
    {
      label: t('support.panel.tableUpload'),
      free: num(free.upload_mib, mb),
      patron: num(patron.upload_mib, mb),
    },
    { label: t('support.panel.tableVideo'), free: video(free), patron: video(patron) },
    {
      label: t('support.panel.tableSenders'),
      free: num(free.video_senders, String),
      patron: num(patron.video_senders, String),
    },
    {
      label: t('support.panel.tableViewers'),
      free: num(free.stream_viewers, String),
      patron: num(patron.stream_viewers, String),
    },
    {
      label: t('support.panel.tableAudio'),
      free: num(free.audio_kbps, kbps),
      patron: num(patron.audio_kbps, kbps),
    },
    {
      label: t('support.panel.tableParties'),
      free: num(free.watch_parties, String),
      patron: num(patron.watch_parties, String),
    },
    {
      label: t('support.panel.tableDownloads'),
      free: num(free.downloads_per_day, perDay),
      patron: num(patron.downloads_per_day, perDay),
    },
  ];
  // A Patron value of zero means the free value still applies; a free value of zero means not included.
  return rows
    .filter((r) => r.free || r.patron)
    .map((r) => ({
      label: r.label,
      free: r.free || t('support.panel.tableNone'),
      patron: r.patron || (r.free ? t('support.panel.tableSame') : t('support.panel.tableNone')),
    }));
}

function PerksTable({ cfg }: { cfg: DonationConfig }) {
  const t = useT();
  return (
    <Card
      id="sup-perks"
      title={t('support.panel.tableTitle')}
      description={t('support.panel.tableDesc')}
    >
      <table className="sup-levels">
        <caption className="us-sr">{t('support.panel.tableCaption')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('support.panel.tablePerk')}</th>
            <th scope="col">{t('support.panel.tableFree')}</th>
            <th scope="col">{t('support.panel.tablePatron')}</th>
          </tr>
        </thead>
        <tbody>
          {perkRows(cfg.free_perks, cfg.patron.perks).map((r) => (
            <tr key={r.label}>
              <th scope="row">{r.label}</th>
              <td>{r.free}</td>
              <td>{r.patron}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

/** Money text for a custom amount typed in dollars; `null` when it is not a usable amount. */
export function parseDollars(raw: string): number | null {
  const body = raw.trim().replace(/^\$/, '');
  if (!/^\d{1,7}(\.\d{1,2})?$/.test(body)) return null;
  const cents = Math.round(Number(body) * 100);
  return cents > 0 ? cents : null;
}

function HostingBar({ cfg }: { cfg: DonationConfig }) {
  const t = useT();
  const { cost_cents, donated_this_month_cents } = cfg.hosting;
  return (
    <Card
      id="sup-hosting"
      title={t('support.panel.hostingTitle')}
      description={t('support.panel.hostingDesc')}
    >
      {cost_cents > 0 ? (
        <>
          <ProgressBar
            value={Math.min(donated_this_month_cents, cost_cents)}
            max={cost_cents}
            label={t('support.panel.hostingBar')}
          />
          <p className="us-muted">
            {t('support.panel.hostingProgress', {
              donated: usd(donated_this_month_cents),
              cost: usd(cost_cents),
            })}
          </p>
        </>
      ) : (
        <p className="us-muted">
          {t('support.panel.hostingDonated', { donated: usd(donated_this_month_cents) })}
        </p>
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
  const t = useT();
  const p = me?.patron;
  let status = t('support.panel.statusNone');
  if (p?.permanent) status = t('support.panel.statusPermanent');
  else if (p?.active && p.until_ms)
    status = t('support.panel.statusUntil', { date: formatDate(p.until_ms, {}) });
  else if (p?.in_grace && p.until_ms)
    status = t('support.panel.statusGrace', {
      date: formatDate(p.until_ms, {}),
      days: cfg.patron.grace_days,
    });
  return (
    <Card
      id="sup-patron"
      title={t('support.panel.patronTitle')}
      description={t('support.panel.patronDesc', { price: usd(cfg.patron.price_cents) })}
      actions={
        p?.active || p?.in_grace ? (
          <Badge variant="success">{t('support.panel.active')}</Badge>
        ) : undefined
      }
    >
      <p role="status">{status}</p>
      <ul className="sup-perks">
        {perkLines(cfg.patron.perks).map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
      <div className="sup-choices" role="group" aria-label={t('support.panel.patronMonths')}>
        {MONTH_CHOICES.filter((m) => m <= cfg.patron.max_months).map((m) => (
          <Button
            key={m}
            variant={m === 1 ? 'primary' : 'secondary'}
            disabled={disabled || busy}
            onClick={() => onPay(m)}
          >
            {t('support.panel.monthChoice', { months: m, price: usd(m * cfg.patron.price_cents) })}
          </Button>
        ))}
      </div>
      <p className="us-muted">{t('support.panel.prepay', { max: cfg.patron.max_months })}</p>
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
  const t = useT();
  const [custom, setCustom] = useState('');
  const cents = parseDollars(custom);
  const bad =
    custom.trim() !== '' &&
    (cents === null || cents < cfg.min_donation_cents || cents > cfg.max_donation_cents);
  return (
    <div className="sup-amount">
      <div className="sup-choices" role="group" aria-label={t('support.panel.amount')}>
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
        <label htmlFor="sup-custom-amount">{t('support.panel.otherAmount')}</label>
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
            {t('support.panel.amountRange', {
              min: usd(cfg.min_donation_cents),
              max: usd(cfg.max_donation_cents),
            })}
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
  const t = useT();
  const guilds = useGuilds().filter((g) => g.id !== '@me' && !g.unavailable);
  const [guildId, setGuildId] = useState('');
  const [anon, setAnon] = useState(false);
  const chosen = guildId || guilds[0]?.id || '';
  return (
    <Card
      id="sup-server"
      title={t('support.panel.serverTitle')}
      description={t('support.panel.serverDesc')}
    >
      <table className="sup-levels">
        <caption className="us-sr">{t('support.panel.levelsCaption')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('support.panel.colLevel')}</th>
            <th scope="col">{t('support.panel.colTotal')}</th>
            <th scope="col">{t('support.panel.colGet')}</th>
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
        <p className="us-muted">{t('support.panel.joinServer')}</p>
      ) : (
        <>
          <div className="sup-custom">
            <label htmlFor="sup-guild">{t('support.panel.server')}</label>
            <select id="sup-guild" value={chosen} onChange={(e) => setGuildId(e.target.value)}>
              {guilds.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name}
                </option>
              ))}
            </select>
          </div>
          <Switch
            label={t('support.panel.anonymous')}
            description={t('support.panel.anonymousDesc')}
            checked={anon}
            onChange={(e) => setAnon(e.target.checked)}
          />
          <AmountPicker
            cfg={cfg}
            cta={t('support.panel.support')}
            disabled={disabled || !chosen}
            onPick={(c) => onPay(chosen, c, anon)}
          />
        </>
      )}
    </Card>
  );
}

function PrefsCard({ me, onChange }: { me: api.Me; onChange: (p: api.Prefs) => void }) {
  const t = useT();
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);
  const save = async (patch: Partial<api.Prefs>) => {
    setError(null);
    try {
      onChange(await api.patchPrefs(patch));
      toast.show({ title: t('support.panel.saved'), duration: 1500 });
    } catch (e) {
      setError(errorText(e));
    }
  };
  const p = me.preferences;
  return (
    <Card
      id="sup-privacy"
      title={t('support.panel.prefsTitle')}
      description={t('support.panel.prefsDesc')}
      actions={<Link to="/support/leaderboards">{t('support.panel.viewLeaderboards')}</Link>}
    >
      <Switch
        label={t('support.panel.hideName')}
        checked={p.hide_name}
        onChange={(e) => void save({ hide_name: e.target.checked })}
      />
      <Switch
        label={t('support.panel.hideAmount')}
        checked={p.hide_amount}
        onChange={(e) => void save({ hide_amount: e.target.checked })}
      />
      <Switch
        label={t('support.panel.hideBadge')}
        checked={p.hide_badge}
        onChange={(e) => void save({ hide_badge: e.target.checked })}
      />
      <Switch
        label={t('support.panel.hideContributions')}
        checked={p.hide_contributions}
        onChange={(e) => void save({ hide_contributions: e.target.checked })}
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
  const t = useT();
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
    <Card
      id="sup-history"
      title={t('support.panel.historyTitle')}
      description={t('support.panel.historyDesc')}
    >
      <Loaded res={res} what={t('support.panel.invoicesWhat')}>
        {(page) => {
          const rows = [...page.items, ...more];
          const cursor = next === undefined ? page.next_before : next;
          return rows.length === 0 ? (
            <p className="us-muted">{t('support.panel.noInvoices')}</p>
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
                        {formatDate(inv.created_at_ms, {})} · {api.STATE_COPY[inv.state]}
                        {inv.explorer_url && (
                          <>
                            {' · '}
                            <a href={inv.explorer_url} target="_blank" rel="noreferrer">
                              {t('support.panel.txLink')}
                              <span className="us-sr">
                                {' '}
                                {t('support.panel.invoiceSr', { id: inv.id })}
                              </span>
                            </a>
                          </>
                        )}
                      </p>
                    </div>
                    <Button variant="secondary" size="sm" onClick={() => onOpen(inv)}>
                      {api.isPaid(inv.state) ? t('support.panel.receipt') : t('support.panel.open')}
                      <span className="us-sr"> {t('support.panel.invoiceSr', { id: inv.id })}</span>
                    </Button>
                  </li>
                ))}
              </ul>
              {cursor && (
                <Button variant="secondary" size="sm" onClick={() => void loadMore()}>
                  {t('support.panel.showMore')}
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

/** Server contributions (LNG-USET-033): the paid invoices that supported a server, newest first. */
export function Contributions({
  onOpen,
  version,
}: {
  onOpen: (inv: Invoice) => void;
  version: number;
}) {
  const t = useT();
  const guilds = useGuilds();
  const res = useResource(() => api.listInvoices({ limit: 50 }));
  const reload = res.reload;
  useEffect(() => {
    if (version > 0) reload();
  }, [version, reload]);
  return (
    <Card
      id="sup-contributions"
      title={t('support.panel.contribTitle')}
      description={t('support.panel.contribDesc')}
    >
      <Loaded res={res} what={t('support.panel.contribWhat')}>
        {(page) => {
          const rows = page.items.filter(
            (i) => i.purpose === 'server_level' && api.isPaid(i.state),
          );
          return rows.length === 0 ? (
            <p className="us-muted">{t('support.panel.contribNone')}</p>
          ) : (
            <ul className="us-list">
              {rows.map((inv) => (
                <li key={inv.id}>
                  <div className="us-row-main">
                    <strong>
                      {guilds.find((g) => g.id === inv.guild_id)?.name ??
                        t('support.panel.contribUnknown')}
                      , {usd(inv.usd_cents)}
                    </strong>
                    <p className="us-muted">
                      {formatDate(inv.created_at_ms, {})}
                      {inv.anonymous ? ` · ${t('support.panel.contribAnon')}` : ''}
                    </p>
                  </div>
                  <Button variant="secondary" size="sm" onClick={() => onOpen(inv)}>
                    {t('support.panel.receipt')}
                    <span className="us-sr"> {t('support.panel.invoiceSr', { id: inv.id })}</span>
                  </Button>
                </li>
              ))}
            </ul>
          );
        }}
      </Loaded>
    </Card>
  );
}

/** The section body; also used by tests and the standalone page. */
export function SupportSection() {
  const t = useT();
  const cfg = useResource(api.getConfig);
  const me = useResource(api.getMe);
  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const [credits, setCredits] = useState(1);

  const create = useCallback(
    async (body: api.CreateInvoice) => {
      setBusy(true);
      setError(null);
      try {
        setInvoice(await api.createInvoice(body));
        setVersion((v) => v + 1);
      } catch (e) {
        setError(errorText(e, t('support.panel.createError')));
      } finally {
        setBusy(false);
      }
    },
    [t],
  );
  const changed = useCallback(() => {
    setVersion((v) => v + 1);
    me.reload();
    cfg.reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <>
      <SectionTitle>{t('support.panel.title')}</SectionTitle>
      <Loaded res={cfg} what={t('support.panel.configWhat')}>
        {(c) => (
          <>
            {!c.enabled && (
              <Notice variant="info" title={t('support.panel.offTitle')}>
                {t('support.panel.offBody')}
              </Notice>
            )}
            {c.enabled && c.rate === null && (
              <Notice variant="warning" title={t('support.panel.noPriceTitle')}>
                {t('support.panel.noPriceBody')}
              </Notice>
            )}
            {error && (
              <Notice variant="danger" role="alert" title={t('support.panel.failedTitle')}>
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
            <PerksTable cfg={c} />
            <Card
              id="sup-donate"
              title={t('support.panel.donateTitle')}
              description={t('support.panel.donateDesc')}
            >
              <AmountPicker
                cfg={c}
                cta={t('support.panel.donate')}
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
              title={t('support.panel.creditsTitle')}
              description={t('support.panel.creditsDesc', {
                price: usd(c.invite_credit_price_cents),
              })}
            >
              <div className="sup-custom">
                <label htmlFor="sup-credit-n">{t('support.panel.credits')}</label>
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
                  {t('support.panel.buyFor', { price: usd(credits * c.invite_credit_price_cents) })}
                </Button>
              </div>
            </Card>
            {me.data && (
              <Card id="sup-limits" title={t('support.panel.limitsTitle')}>
                <dl>
                  <Field label={t('support.panel.uploadSize')}>
                    {t('support.panel.mbPerFile', { mb: me.data.limits.upload_bytes / 1048576 })}
                  </Field>
                  <Field label={t('support.panel.downloads')}>
                    {me.data.limits.downloads_per_day > 0
                      ? t('support.panel.perDay', { n: me.data.limits.downloads_per_day })
                      : t('support.panel.patronsOnly')}
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
      <Contributions version={version} onOpen={setInvoice} />
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
