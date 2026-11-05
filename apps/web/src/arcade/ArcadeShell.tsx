// ARC-UI-010: the Arcade's room chrome, in the family of the other rooms (The Study, the Cinema, the Library): the
// `SIDE ROOM / The Lounge · The Arcade · The Study` bar with the Arcade current, then the room's own row of pill
// links (Playable now, Server cartridges, Coming soon) and its "+ List a cartridge" action. It follows the member's
// theme through the `--ui-*` tokens, as The Study does. Shared by the real page and the Showroom's sample Arcade.
import type { ReactNode } from 'react';
import { useT } from '../i18n';
import { RoomsNav } from '../study/RoomsNav';

export interface HeroLink {
  href: string;
  label: string;
}

export function ArcadeShell({
  links,
  action,
  testId,
  children,
}: {
  links: readonly HeroLink[];
  /** A control at the end of the pill row (the page's "+ List a cartridge"). */
  action?: ReactNode;
  testId?: string;
  children: ReactNode;
}) {
  const t = useT();
  return (
    <div className="arc-root">
      <header className="arc-roombar">
        <span className="arc-roombar-brand">{t('arcade.page.brand')}</span>
        <span aria-hidden="true" className="arc-roombar-slash">
          /
        </span>
        <RoomsNav current="arcade" className="arc-roombar-nav" linkClassName="arc-roombar-link" />
      </header>
      {(links.length > 0 || action) && (
        <div className="arc-tabs">
          {links.length > 0 && (
            <nav aria-label={t('arcade.page.nav')} className="arc-nav">
              {links.map((l) => (
                <a key={l.href} href={l.href}>
                  {l.label}
                </a>
              ))}
            </nav>
          )}
          {action && <div className="arc-tabs-action">{action}</div>}
        </div>
      )}
      <main className="arc-page" data-testid={testId}>
        {children}
      </main>
    </div>
  );
}
