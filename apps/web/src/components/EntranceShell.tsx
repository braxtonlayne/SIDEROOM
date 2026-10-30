import { ThemeProvider } from '@sideroom/ui';
import { useEntranceThemes } from '../lib/entranceTheme';
import type { ReactNode } from 'react';
import { useT } from '../i18n/react';
import { DoorIcon } from './icons';
import { SkipLink } from './SkipLink';
import './entrance.css';

export function Logo() {
  const t = useT();
  return (
    <a href="/" className="ent-logo" aria-label={t('entrance.shell.home')}>
      <DoorIcon width={36} height={36} />
      <span className="ent-wordmark">
        SIDE ROOM<span className="ent-dot">.</span>
      </span>
    </a>
  );
}

export interface EntranceShellProps {
  /** Right-hand header link. */
  nav: 'member-access' | 'back-to-entrance' | 'none';
  /** `'House notes'` links to /house-notes; anything else is plain text. Defaults to "By invitation only". */
  footerRight?: string;
  children: ReactNode;
}

/** Shared chrome for every entrance page: theme root, header, footer. */
export function EntranceShell({ nav, footerRight, children }: EntranceShellProps) {
  const t = useT();
  const houseNotes = footerRight === 'House notes';
  // Signed-out pages only know this browser's saved choice of Entrance theme (lib/entranceTheme.ts).
  const perSection = useEntranceThemes();
  return (
    <ThemeProvider
      section="entrance"
      platformDefault="sideroom"
      perSection={perSection}
      className="ent-root"
    >
      <SkipLink />
      <header className="ent-header">
        <Logo />
        <p className="ent-tagline">{t('entrance.shell.tagline')}</p>
        <div className="ent-nav">
          {nav === 'member-access' && (
            <a href="/login" className="ent-navlink">
              {t('entrance.shell.memberAccess')} <span aria-hidden="true">↗</span>
            </a>
          )}
          {nav === 'back-to-entrance' && (
            <a href="/" className="ent-navlink">
              <span aria-hidden="true">↖</span> {t('entrance.shell.backToEntrance')}
            </a>
          )}
        </div>
      </header>
      {children}
      <footer className="ent-footer">
        <p className="ent-footer-line">
          {t('entrance.shell.footerLine')} <em>{t('entrance.shell.footerAccent')}</em>
        </p>
        <p className="ent-footer-meta">
          {houseNotes ? (
            <a href="/house-notes">{t('entrance.shell.houseNotes')}</a>
          ) : (
            <span>{footerRight ?? t('entrance.shell.invitationOnly')}</span>
          )}
          <span aria-hidden="true">/</span>
          <span>{t('entrance.shell.copyright', { year: String(new Date().getFullYear()) })}</span>
        </p>
      </footer>
    </ThemeProvider>
  );
}
