// Cinema section chrome (CIN-UI-001/002/004/005): section top bar, cinema header with nav, search and avatar
// menu, footer, and the phone layout (hamburger drawer).
import { SkipLink } from '../components/SkipLink';
import {
  Avatar,
  Drawer,
  DoorLogo,
  DropdownMenu,
  Icon,
  MOBILE_QUERY,
  useMediaQuery,
  type MenuEntry,
} from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { getMe } from '../settings/user/api';
import { useOpenUserSettings } from '../settings/user/nav';
import { keys } from './api';
import { SearchOverlay } from './Search';

/** `SIDE ROOM / The lounge · The cinema · The library` with the cinema active (CIN-UI-001). No "board". */
export function SectionBar() {
  const t = useT();
  return (
    <header className="cin-sectionbar">
      <span className="cin-sectionbar-brand">SIDE ROOM</span>
      <span aria-hidden="true" className="cin-sectionbar-slash">
        /
      </span>
      <nav aria-label={t('cinema.shell.sections')} className="cin-sectionbar-nav">
        <a href="/channels" className="cin-sectionbar-link">
          {t('cinema.shell.lounge')}
        </a>
        <a href="/cinema" className="cin-sectionbar-link" aria-current="page">
          {t('cinema.shell.cinema')}
        </a>
        <a href="/library" className="cin-sectionbar-link">
          {t('cinema.shell.library')}
        </a>
      </nav>
    </header>
  );
}

interface NavItem {
  href: string;
  /** Catalog key of the label. */
  label: string;
  /** Marker after the label: a sparkle for anime, a dot for live TV. */
  mark?: 'sparkle' | 'dot';
  exact?: boolean;
  anime?: boolean;
}

export const NAV: NavItem[] = [
  { href: '/cinema', label: 'cinema.shell.navDiscover', exact: true },
  { href: '/cinema/movies', label: 'cinema.shell.navMovies' },
  { href: '/cinema/series', label: 'cinema.shell.navSeries' },
  { href: '/cinema/anime', label: 'cinema.shell.navAnime', mark: 'sparkle', anime: true },
  { href: '/cinema/tv', label: 'cinema.shell.navLiveTv', mark: 'dot' },
  { href: '/cinema/my-list', label: 'cinema.shell.navMyList' },
  { href: '/cinema/submit', label: 'cinema.submit.nav' },
];

const isActive = (path: string, i: NavItem) =>
  i.exact ? path === i.href : path === i.href || path.startsWith(`${i.href}/`);

function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
  const t = useT();
  const path = useRouterState({ select: (s) => s.location.pathname });
  return (
    <ul className="cin-nav-list">
      {NAV.map((i) => (
        <li key={i.href}>
          <a
            href={i.href}
            className="cin-nav-link"
            data-anime={i.anime || undefined}
            aria-current={isActive(path, i) ? 'page' : undefined}
            onClick={onNavigate}
          >
            {t(i.label)}
            {i.mark && (
              <span aria-hidden="true" className="cin-nav-mark">
                {i.mark === 'sparkle' ? ' ✦' : ' •'}
              </span>
            )}
          </a>
        </li>
      ))}
    </ul>
  );
}

function AvatarMenu() {
  const t = useT();
  const navigate = useNavigate();
  const openSettings = useOpenUserSettings();
  const me = useQuery({ queryKey: keys.me(), queryFn: getMe, staleTime: 5 * 60_000 });
  const name =
    me.data?.global_name ?? me.data?.display_name ?? me.data?.username ?? t('cinema.shell.you');
  // D-367: "My uploads" opens the Cinema's own submissions list and "Upload" the same list with the form open.
  // "Requests" still goes to the Library board until the Cinema has a requests board of its own.
  const items: MenuEntry[] = [
    { type: 'label', label: name },
    { label: t('cinema.shell.profile'), onSelect: () => openSettings() },
    {
      label: t('cinema.shell.myUploads'),
      onSelect: () => void navigate({ to: '/cinema/submit' }),
    },
    {
      label: t('cinema.shell.upload'),
      onSelect: () => void navigate({ to: '/cinema/submit', search: { new: 1 } }),
    },
    {
      label: t('cinema.shell.requests'),
      onSelect: () => void navigate({ to: '/library/requests' }),
    },
    { label: t('cinema.shell.settings'), onSelect: () => openSettings() },
    { type: 'separator' },
    { label: t('cinema.shell.backToLounge'), onSelect: () => void navigate({ to: '/channels' }) },
  ];
  return (
    <DropdownMenu
      label={t('cinema.shell.account')}
      placement="bottom-end"
      items={items}
      trigger={
        <button type="button" className="cin-avatar-btn" aria-label={t('cinema.shell.accountMenu')}>
          <Avatar name={name} size={32} />
        </button>
      }
    />
  );
}

export function useRequireSession() {
  const navigate = useNavigate();
  const here = useRouterState({ select: (s) => s.location.href });
  const sent = useRef(false);
  const me = useQuery({ queryKey: keys.me(), queryFn: getMe, staleTime: 5 * 60_000, retry: false });
  const unauthorized = me.error instanceof ApiError && me.error.status === 401;
  useEffect(() => {
    if (!unauthorized || sent.current) return;
    sent.current = true;
    void navigate({ to: '/login', search: { next: here } });
  }, [unauthorized, navigate, here]);
  return unauthorized;
}

export function CinemaHeader({ onSearch, onMenu }: { onSearch: () => void; onMenu?: () => void }) {
  const t = useT();
  return (
    <div className="cin-header">
      {onMenu && (
        <button
          type="button"
          className="cin-icon-btn"
          aria-label={t('cinema.shell.openMenu')}
          onClick={onMenu}
        >
          <Icon name="menu" size={20} />
        </button>
      )}
      <a href="/cinema" className="cin-logo" aria-label={t('cinema.shell.logoLabel')}>
        <DoorLogo size={32} />
        <span className="cin-logo-text">
          <span className="cin-logo-name">SIDE ROOM</span>
          <span className="cin-logo-sub" aria-hidden="true" data-text="C I N E M A" />
        </span>
      </a>
      {!onMenu && (
        <nav aria-label={t('cinema.shell.navLabel')} className="cin-nav">
          <NavLinks />
        </nav>
      )}
      <div className="cin-header-tools">
        <button
          type="button"
          className="cin-icon-btn"
          aria-label={t('cinema.shell.search')}
          aria-keyshortcuts="/"
          onClick={onSearch}
        >
          <Icon name="search" size={20} />
        </button>
        <AvatarMenu />
      </div>
    </div>
  );
}

export function CinemaFooter() {
  const t = useT();
  return (
    <footer className="cin-footer">
      <span className="cin-footer-brand">
        Side Room <em>Cinema</em>
      </span>
      <span className="cin-footer-tag">{t('cinema.shell.tagline')}</span>
      <a href="/support/leaderboards?tab=movies" className="cin-footer-link">
        {t('cinema.shell.contributorBoards')}
      </a>
      <a href="/house-notes" className="cin-footer-link">
        {t('cinema.shell.aboutLibrary')}
      </a>
    </footer>
  );
}

/** The page frame around every Cinema page except the player. */
export function CinemaShell({ children }: { children?: ReactNode }) {
  const tr = useT();
  const mobile = useMediaQuery(MOBILE_QUERY);
  const [drawer, setDrawer] = useState(false);
  const [search, setSearch] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target;
      const typing =
        t instanceof HTMLElement &&
        (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if (e.key === '/' && !typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setSearch(true);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <div className="cin-frame" data-mobile={mobile || undefined}>
      <SkipLink />
      <SectionBar />
      <CinemaHeader
        onSearch={() => setSearch(true)}
        onMenu={mobile ? () => setDrawer(true) : undefined}
      />
      <main className="cin-main" id="cin-main">
        {children ?? <Outlet />}
      </main>
      <CinemaFooter />
      {mobile && (
        <Drawer
          open={drawer}
          onClose={() => setDrawer(false)}
          title={tr('cinema.shell.cinema')}
          side="left"
        >
          <nav aria-label={tr('cinema.shell.menuLabel')}>
            <NavLinks onNavigate={() => setDrawer(false)} />
          </nav>
        </Drawer>
      )}
      <SearchOverlay open={search} onClose={() => setSearch(false)} />
    </div>
  );
}
