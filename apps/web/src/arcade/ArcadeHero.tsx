// ARC-UI-010: the Arcade's header: title, one-line pitch, how to start a game and the jump links. Shared by the real
// page and the Showroom's sample Arcade so both read as the same room.
import { Icon } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { useT } from '../i18n';

export interface HeroLink {
  href: string;
  label: string;
}

/** A decorative arcade-cabinet motif in the theme's accent (tokens only, so it follows the theme). */
function HeroMotif() {
  return (
    <svg className="arc-hero-motif" viewBox="0 0 200 160" aria-hidden="true" focusable="false">
      <g fill="currentColor">
        <rect x="58" y="10" width="84" height="140" rx="10" opacity="0.18" />
        <rect x="70" y="24" width="60" height="46" rx="4" opacity="0.5" />
        <rect x="84" y="36" width="8" height="8" opacity="0.9" />
        <rect x="108" y="36" width="8" height="8" opacity="0.9" />
        <rect x="80" y="52" width="40" height="6" opacity="0.9" />
        <rect x="72" y="88" width="56" height="10" rx="5" opacity="0.35" />
        <circle cx="86" cy="84" r="7" opacity="0.8" />
        <rect x="84" y="84" width="4" height="12" opacity="0.8" />
        <circle cx="108" cy="92" r="4" opacity="0.7" />
        <circle cx="120" cy="88" r="4" opacity="0.7" />
        <rect x="14" y="40" width="10" height="10" opacity="0.35" />
        <rect x="30" y="110" width="8" height="8" opacity="0.25" />
        <rect x="168" y="30" width="12" height="12" opacity="0.3" />
        <rect x="160" y="118" width="8" height="8" opacity="0.4" />
        <rect x="182" y="80" width="6" height="6" opacity="0.5" />
      </g>
    </svg>
  );
}

export function ArcadeHero({
  links,
  children,
}: {
  links: readonly HeroLink[];
  children?: ReactNode;
}) {
  const t = useT();
  return (
    <header className="arc-hero">
      <div className="arc-hero-main">
        <p className="arc-eyebrow">{t('arcade.hero.eyebrow')}</p>
        <h1 className="arc-hero-title">{t('arcade.page.title')}</h1>
        <p className="arc-hero-pitch">{t('arcade.hero.pitch')}</p>
        {children}
      </div>
      <HeroMotif />
      <section className="arc-how" aria-labelledby="arc-how">
        <h2 id="arc-how" className="arc-how-heading">
          {t('arcade.hero.howHeading')}
        </h2>
        <ol className="arc-how-steps">
          <li>
            <span className="arc-how-icon">
              <Icon name="plus" size={20} />
            </span>
            <span>
              <strong>{t('arcade.hero.howMenuTitle')}</strong>
              <span className="arc-how-text">{t('arcade.hero.howMenu')}</span>
            </span>
          </li>
          <li>
            <span className="arc-how-icon">
              <Icon name="phone" size={20} />
            </span>
            <span>
              <strong>{t('arcade.hero.howCallTitle')}</strong>
              <span className="arc-how-text">{t('arcade.hero.howCall')}</span>
            </span>
          </li>
        </ol>
      </section>
      {links.length > 0 && (
        <nav aria-label={t('arcade.page.nav')} className="arc-nav">
          {links.map((l) => (
            <a key={l.href} href={l.href}>
              {l.label}
            </a>
          ))}
        </nav>
      )}
    </header>
  );
}
