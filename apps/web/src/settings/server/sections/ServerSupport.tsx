// Server Settings > Server Support (DON-UI-004), the replacement for Discord's "Boost Perks": the server's
// level and monthly pool, what the level gives every member, the top contributors, a contribute flow for the
// signed-in member, and (owner only) hiding the server from the public leaderboard.
import { Badge, Notice, ProgressBar, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { formatDate, useT } from '../../../i18n';
import * as api from '../../../support/api';
import { errorText, usd, type DonationConfig, type Invoice } from '../../../support/api';
import { InvoiceModal } from '../../../support/InvoiceModal';
import { AmountPicker, perkLines } from '../../../support/SupportPanel';
import '../../../support/support.css';
import { useResource } from '../hooks';
import { Async, SectionHeader } from '../ui';
import type { SectionProps } from '../ServerSettings';

export function ServerSupportSection({ guildId }: SectionProps) {
  const t = useT();
  const res = useResource(`support-${guildId}`, async () => ({
    support: await api.getGuildSupport(guildId),
    cfg: await api.getConfig(),
  }));
  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [anon, setAnon] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const contribute = async (cents: number) => {
    setBusy(true);
    setError(null);
    try {
      setInvoice(
        await api.createInvoice({
          purpose: 'server_level',
          guild_id: guildId,
          usd_cents: cents,
          anonymous: anon,
        }),
      );
    } catch (e) {
      setError(errorText(e, t('settings.serverSupport.invoiceFailed')));
    } finally {
      setBusy(false);
    }
  };
  const setHidden = async (hidden: boolean) => {
    setError(null);
    try {
      await api.patchGuildSupport(guildId, hidden);
      await res.reload();
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <>
      <SectionHeader
        title={t('settings.serverSupport.title')}
        description={t('settings.serverSupport.description')}
      />
      <Async res={res} label={t('settings.serverSupport.loading')}>
        {({ support: s, cfg }: { support: api.GuildSupport; cfg: DonationConfig }) => (
          <>
            <section className="ss-card" aria-labelledby="sup-level-h">
              <h2 id="sup-level-h" className="ss-card-title">
                {t('settings.serverSupport.level', { level: s.level })}{' '}
                <Badge variant={s.level > 0 ? 'success' : 'neutral'}>
                  {s.level > 0
                    ? t('settings.serverSupport.active')
                    : t('settings.serverSupport.free')}
                </Badge>
              </h2>
              {s.next_threshold_cents !== null ? (
                <>
                  <ProgressBar
                    value={Math.min(s.pool_cents, s.next_threshold_cents)}
                    max={s.next_threshold_cents}
                    label={t('settings.serverSupport.poolProgress')}
                  />
                  <p className="sup-muted">
                    {t('settings.serverSupport.poolToNext', {
                      pool: usd(s.pool_cents),
                      target: usd(s.next_threshold_cents),
                      level: s.level + 1,
                      days: s.pool_window_days,
                    })}
                  </p>
                </>
              ) : (
                <p className="sup-muted">
                  {t('settings.serverSupport.poolTop', {
                    pool: usd(s.pool_cents),
                    days: s.pool_window_days,
                  })}
                </p>
              )}
              {s.grace_until_ms && (
                <Notice variant="warning" title={t('settings.serverSupport.graceTitle')}>
                  {t('settings.serverSupport.graceBody', {
                    date: formatDate(s.grace_until_ms, {}),
                  })}
                </Notice>
              )}
              <ul className="sup-perks">
                {perkLines(s.perks).map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
            </section>

            <section className="ss-card" aria-labelledby="sup-contrib-h">
              <h2 id="sup-contrib-h" className="ss-card-title">
                {t('settings.serverSupport.topContributors')}
              </h2>
              {s.top_contributors.length === 0 ? (
                <p className="sup-muted">
                  {t('settings.serverSupport.noContributions', { days: s.pool_window_days })}
                </p>
              ) : (
                <ol className="sup-board" aria-label={t('settings.serverSupport.topContributors')}>
                  {s.top_contributors.map((c, i) => (
                    <li key={`${i}-${c.user_id ?? 'anon'}`}>
                      <span className="sup-rank">#{i + 1}</span>
                      <span className="sup-name">{c.name}</span>
                      <span className="sup-muted">{c.cents === null ? '' : usd(c.cents)}</span>
                    </li>
                  ))}
                </ol>
              )}
            </section>

            <section className="ss-card" aria-labelledby="sup-give-h">
              <h2 id="sup-give-h" className="ss-card-title">
                {t('settings.serverSupport.supportTitle')}
              </h2>
              {!cfg.enabled ? (
                <p className="sup-muted">{t('settings.serverSupport.donationsOff')}</p>
              ) : (
                <>
                  <Switch
                    label={t('settings.serverSupport.anonymous')}
                    checked={anon}
                    onChange={(e) => setAnon(e.target.checked)}
                  />
                  <AmountPicker
                    cfg={cfg}
                    cta={t('settings.serverSupport.cta')}
                    disabled={busy}
                    onPick={(c) => void contribute(c)}
                  />
                </>
              )}
            </section>

            {s.is_owner && (
              <section className="ss-card" aria-labelledby="sup-hide-h">
                <h2 id="sup-hide-h" className="ss-card-title">
                  {t('settings.serverSupport.leaderboard')}
                </h2>
                <Switch
                  label={t('settings.serverSupport.hide')}
                  checked={s.hidden_from_leaderboard}
                  onChange={(e) => void setHidden(e.target.checked)}
                />
              </section>
            )}
          </>
        )}
      </Async>
      {error && (
        <Notice variant="danger" role="alert">
          {error}
        </Notice>
      )}
      {invoice && (
        <InvoiceModal
          invoice={invoice}
          onClose={() => {
            setInvoice(null);
            void res.reload();
          }}
        />
      )}
    </>
  );
}
