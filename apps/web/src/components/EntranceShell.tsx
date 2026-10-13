import { Link } from '@tanstack/react-router';
import { ThemeProvider } from '@sideroom/ui';
import { useSavedPrefs } from '../settings/user/prefs';
import type { ReactNode } from 'react';
import { DoorIcon } from './icons';
import './entrance.css';

export function Logo() {
  return (
    <Link to="/" className="ent-logo" aria-label="SIDE ROOM home">
      <DoorIcon width={36} height={36} />
      <span className="ent-wordmark">
        SIDE ROOM<span className="ent-dot">.</span>
      </span>
    </Link>
  );
}

export interface EntranceShellProps {
  /** Right-hand header link. */
  nav: 'member-access' | 'back-to-entrance' | 'none';
  footerRight?: string;
  children: ReactNode;
}

/** Shared chrome for every entrance page: theme root, header, footer. */
export function EntranceShell({
  nav,
  footerRight = 'By invitation only',
  children,
}: EntranceShellProps) {
  // Signed-out pages only know this browser's saved choice of Entrance theme.
  const { sectionThemes } = useSavedPrefs();
  return (
    <ThemeProvider
      section="entrance"
      platformDefault="sideroom"
      perSection={sectionThemes}
      className="ent-root"
    >
      <header className="ent-header">
        <Logo />
        <p className="ent-tagline">A private corner of the internet</p>
        <div className="ent-nav">
          {nav === 'member-access' && (
            <Link to="/login" className="ent-navlink">
              Member access <span aria-hidden="true">↗</span>
            </Link>
          )}
          {nav === 'back-to-entrance' && (
            <Link to="/" className="ent-navlink">
              <span aria-hidden="true">↖</span> Back to the entrance
            </Link>
          )}
        </div>
      </header>
      {children}
      <footer className="ent-footer">
        <p className="ent-footer-line">
          Not everywhere. <em>Just here.</em>
        </p>
        <p className="ent-footer-meta">
          {footerRight === 'House notes' ? (
            <Link to="/house-notes">House notes</Link>
          ) : (
            <span>{footerRight}</span>
          )}
          <span aria-hidden="true">/</span>
          <span>SIDE ROOM © {new Date().getFullYear()}</span>
        </p>
      </footer>
    </ThemeProvider>
  );
}
