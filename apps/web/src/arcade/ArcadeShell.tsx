// ARC-UI-010: the Arcade's frame, in the Lounge's own chrome so moving between rooms feels like one app: the global
// section bar (with the Arcade as the current room), the desktop title strip, then a Lounge header bar (icon, title,
// jump links) and an info notice that says how to start a game. Shared by the real page and the Showroom's Arcade.
import { Icon, Notice } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { useT } from '../i18n';
import { SectionBar, TitleStrip } from '../lounge/Chrome';
import '../lounge/lounge.css';

export interface HeroLink {
  href: string;
  label: string;
}

export function ArcadeShell({
  links,
  testId,
  children,
}: {
  links: readonly HeroLink[];
  testId?: string;
  children: ReactNode;
}) {
  const t = useT();
  return (
    <div className="arc-root">
      <div className="arc-chrome">
        <SectionBar current="arcade" />
        <div className="arc-titlestrip">
          <TitleStrip title={t('arcade.page.title')} />
        </div>
      </div>
      <main className="arc-page" data-testid={testId}>
        <header className="lng-header arc-header">
          <Icon name="play" size={24} />
          <h1 className="lng-header-title">{t('arcade.page.title')}</h1>
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
        <div className="arc-body">
          <Notice variant="info" title={t('arcade.hero.pitch')} className="arc-how">
            <ul className="arc-how-steps">
              <li>
                <Icon name="plus" size={16} />
                <span>
                  <strong>{t('arcade.hero.howMenuTitle')}</strong> {t('arcade.hero.howMenu')}
                </span>
              </li>
              <li>
                <Icon name="phone" size={16} />
                <span>
                  <strong>{t('arcade.hero.howCallTitle')}</strong> {t('arcade.hero.howCall')}
                </span>
              </li>
            </ul>
          </Notice>
          {children}
        </div>
      </main>
    </div>
  );
}
