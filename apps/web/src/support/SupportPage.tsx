// /support (DON-UI-001): the page that explains why SIDE ROOM asks for donations, shows this month's
// hosting cost against what was donated, the Patron card, what Server Levels are, invite credits and a
// link to the leaderboards, then the help answers that used to be the whole page. It only reads the
// donation APIs; paying is done in Settings > Support / Patron, where the invoice dialog lives.
import { Badge, Notice, ThemeProvider, ToastProvider } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { formatDate, useT } from '../i18n';
import { useResource } from '../settings/user/api';
import { Card, Loaded } from '../settings/user/parts';
import * as api from './api';
import { usd, type DonationConfig } from './api';
import { HostingBar, perkLines } from './SupportPanel';
import './support.css';
import '../settings/user/user-settings.css';

function PatronSummary({ cfg, me }: { cfg: DonationConfig; me: api.Me | undefined }) {
  const t = useT();
  const p = me?.patron;
  let status: string | null = me ? t('support.panel.statusNone') : null;
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
      id="sup-page-patron"
      title={t('support.panel.patronTitle')}
      description={t('support.panel.patronDesc', { price: usd(cfg.patron.price_cents) })}
      actions={
        p?.active || p?.in_grace ? (
          <Badge variant="success">{t('support.panel.active')}</Badge>
        ) : undefined
      }
    >
      {status && <p role="status">{status}</p>}
      <ul className="sup-perks">
        {perkLines(cfg.patron.perks).map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
      <p>
        {cfg.enabled && (
          <Link to="/settings/user/$section" params={{ section: 'support' }}>
            {p?.active ? t('support.page.patronExtend') : t('support.page.patronCta')}
          </Link>
        )}
      </p>
    </Card>
  );
}

function LevelsExplainer({ cfg }: { cfg: DonationConfig }) {
  const t = useT();
  return (
    <Card
      id="sup-page-levels"
      title={t('support.page.levelsTitle')}
      description={t('support.page.levelsBody')}
    >
      <ol className="sup-levels-list">
        {cfg.levels.map((l) => (
          <li key={l.level}>
            <strong>
              {t('support.page.levelHead', { level: l.level, amount: usd(l.threshold_cents) })}
            </strong>
            <ul className="sup-perks">
              {perkLines(l.perks).map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </li>
        ))}
      </ol>
      <p className="us-muted">{t('support.page.levelsNote')}</p>
    </Card>
  );
}

function Body() {
  const t = useT();
  const cfg = useResource(api.getConfig);
  const me = useResource(api.getMe);
  const signedOut = (cfg.error as { status?: number } | undefined)?.status === 401;
  if (signedOut)
    return (
      <Notice variant="info" title={t('support.page.signInTitle')}>
        {t('support.page.signInBody')} <a href="/login?next=%2Fsupport">{t('support.page.signInLink')}</a>
      </Notice>
    );
  return (
    <Loaded res={cfg} what={t('support.panel.configWhat')}>
      {(c) => (
        <>
          {!c.enabled && (
            <Notice variant="info" title={t('support.panel.offTitle')}>
              {t('support.panel.offBody')}
            </Notice>
          )}
          <HostingBar cfg={c} />
          <PatronSummary cfg={c} me={me.data} />
          <LevelsExplainer cfg={c} />
          <Card
            id="sup-page-credits"
            title={t('support.panel.creditsTitle')}
            description={t('support.panel.creditsDesc', {
              price: usd(c.invite_credit_price_cents),
            })}
          >
            <p>
              <Link to="/settings/user/$section" params={{ section: 'support' }}>
                {t('support.page.openSettings')}
              </Link>
            </p>
          </Card>
        </>
      )}
    </Loaded>
  );
}

export function SupportPage() {
  const t = useT();
  return (
    <ThemeProvider section="lounge" className="ss-root">
      <ToastProvider>
        <main className="sup-page">
          <h1 className="us-title">{t('support.page.title')}</h1>
          <section aria-labelledby="sup-why-h" className="us-card">
            <h2 className="us-card-title" id="sup-why-h">
              {t('support.page.whyTitle')}
            </h2>
            <p>{t('support.page.whyBody')}</p>
            <p className="us-muted">{t('support.page.whyNoAds')}</p>
          </section>
          <Body />
          <p>
            <Link to="/support/leaderboards">{t('support.page.leaderboards')}</Link>
          </p>
          <p className="us-muted">
            {t('support.page.needHelp')} <Link to="/help">{t('support.page.helpLink')}</Link>
          </p>
        </main>
      </ToastProvider>
    </ThemeProvider>
  );
}
