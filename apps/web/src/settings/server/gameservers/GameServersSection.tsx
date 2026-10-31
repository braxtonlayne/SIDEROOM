// Guild Settings > Game Servers (GSV-PAGE-001, D-383). ENTIRELY COSMETIC: the feature is deferred, so this is a 90s
// "COMING SOON" splash and a mock-up of what the page could look like. Nothing here is interactive, nothing is read
// or written, and no request is made. Colours and fonts come from tokens only (scripts/check-css-tokens.mjs).
import { useMediaQuery } from '@sideroom/ui';
import { useT } from '../../../i18n';
import type { SectionProps } from '../ServerSettings';
import { SectionHeader } from '../ui';
import './gameservers.css';

/** The hazard-stripe strip with its sign. */
export function UnderConstruction() {
  const t = useT();
  return (
    <div className="gsv-construction">
      <div className="gsv-hazard" aria-hidden="true" />
      <p className="gsv-sign">{t('settings.gameServers.construction')}</p>
      <div className="gsv-hazard" aria-hidden="true" />
    </div>
  );
}

/** A scrolling line of text; under `prefers-reduced-motion` it stands still and wraps instead. */
export function Marquee({ children }: { children: string }) {
  const reduced = useMediaQuery('(prefers-reduced-motion: reduce)');
  return (
    <div
      className="gsv-marquee"
      data-testid="gsv-marquee"
      data-motion={reduced ? 'reduced' : 'full'}
    >
      <span className="gsv-marquee-track">{children}</span>
    </div>
  );
}

/** The visitor counter that counts nothing: seven zeros, always. */
export function VisitorCounter() {
  const t = useT();
  return (
    <p className="gsv-counter">
      <span>{t('settings.gameServers.counterCaption')}</span>{' '}
      <span
        className="gsv-counter-digits"
        role="img"
        aria-label={t('settings.gameServers.counterLabel')}
      >
        {'0000000'.split('').map((d, i) => (
          <span key={i} className="gsv-digit" aria-hidden="true">
            {d}
          </span>
        ))}
      </span>
    </p>
  );
}

function ComingSoonPill() {
  const t = useT();
  return <span className="gsv-pill gsv-pill-soon">{t('settings.gameServers.comingSoon')}</span>;
}

function Row({
  name,
  status,
  tone,
  meta,
}: {
  name: string;
  status: string;
  tone: string;
  meta: string;
}) {
  return (
    <li className="gsv-row">
      <span className="gsv-row-name">{name}</span>
      <span className="gsv-row-meta">{meta}</span>
      <span className={`gsv-pill gsv-pill-${tone}`}>{status}</span>
    </li>
  );
}

/** A greyed, inert picture of a server list and a console strip. Hidden from assistive tech; the caption says what it is. */
function Preview({ tier }: { tier: 'byoc' | 'managed' }) {
  const t = useT();
  const byoc = tier === 'byoc';
  return (
    <figure className="gsv-preview">
      <div className="gsv-preview-body" aria-hidden="true">
        <ul className="gsv-list">
          <Row
            name={t('settings.gameServers.mockNameA')}
            status={t('settings.gameServers.mockRunning')}
            tone="ok"
            meta={t('settings.gameServers.mockPlayers', { n: 3, max: 8 })}
          />
          <Row
            name={t('settings.gameServers.mockNameB')}
            status={
              byoc ? t('settings.gameServers.mockStopped') : t('settings.gameServers.mockStarting')
            }
            tone={byoc ? 'off' : 'wait'}
            meta={t('settings.gameServers.mockPlayers', { n: 0, max: 12 })}
          />
        </ul>
        <pre className="gsv-console">
          {t(
            byoc
              ? 'settings.gameServers.mockConsoleByoc'
              : 'settings.gameServers.mockConsoleManaged',
          )}
        </pre>
      </div>
      <figcaption className="gsv-caption">{t('settings.gameServers.previewCaption')}</figcaption>
    </figure>
  );
}

/** The two cards: "Host it yourself (BYOC)" and "Managed servers". Each says the same thing: coming soon. */
export function GameServerCards() {
  const t = useT();
  return (
    <div className="gsv-cards">
      <section className="gsv-card" aria-labelledby="gsv-byoc">
        <header className="gsv-card-head">
          <h3 id="gsv-byoc" className="gsv-card-title">
            {t('settings.gameServers.byocTitle')}
          </h3>
          <ComingSoonPill />
        </header>
        <p className="gsv-card-text">{t('settings.gameServers.byocText')}</p>
        <Preview tier="byoc" />
      </section>
      <section className="gsv-card" aria-labelledby="gsv-managed">
        <header className="gsv-card-head">
          <h3 id="gsv-managed" className="gsv-card-title">
            {t('settings.gameServers.managedTitle')}
          </h3>
          <ComingSoonPill />
        </header>
        <p className="gsv-card-text">{t('settings.gameServers.managedText')}</p>
        <Preview tier="managed" />
      </section>
    </div>
  );
}

export function GameServersSection(_props: SectionProps) {
  const t = useT();
  return (
    <div className="gsv-root">
      <SectionHeader
        title={t('settings.gameServers.title')}
        description={t('settings.gameServers.intro')}
      />
      <UnderConstruction />
      <h2 className="gsv-soon">{t('settings.gameServers.splash')}</h2>
      <Marquee>{t('settings.gameServers.marquee')}</Marquee>
      <GameServerCards />
      <VisitorCounter />
    </div>
  );
}
