// Cinema section chrome (CIN-UI-001/002/004/005): section top bar, cinema header with nav, search and avatar
// menu, footer, and the phone layout (hamburger drawer).
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
import { getMe } from '../settings/user/api';
import { useOpenUserSettings } from '../settings/user/nav';
import { keys } from './api';
import { SearchOverlay } from './Search';

/** `SIDE ROOM / The lounge · The cinema · The library` with the cinema active (CIN-UI-001). No "board". */
export function SectionBar() {
  return (
    <header className="cin-sectionbar">
      <span className="cin-sectionbar-brand">SIDE ROOM</span>
      <span aria-hidden="true" className="cin-sectionbar-slash">
        /
      </span>
      <nav aria-label="Sections" className="cin-sectionbar-nav">
        <a href="/channels" className="cin-sectionbar-link">
          The lounge
        </a>
        <a href="/cinema" className="cin-sectionbar-link" aria-current="page">
          The cinema
        </a>
        <a href="/library" className="cin-sectionbar-link">
          The library
        </a>
      </nav>
    </header>
  );
}

interface NavItem {
  href: string;
  label: string;
  /** Marker after the label: a sparkle for anime, a dot for live TV. */
  mark?: 'sparkle' | 'dot';
  exact?: boolean;
  anime?: boolean;
}

export const NAV: NavItem[] = [
  { href: '/cinema', label: 'Discover', exact: true },
  { href: '/cinema/movies', label: 'Movies' },
  { href: '/cinema/series', label: 'Series' },
  { href: '/cinema/anime', label: 'Anime', mark: 'sparkle', anime: true },
  { href: '/cinema/tv', label: 'Live TV', mark: 'dot' },
  { href: '/cinema/my-list', label: 'My list' },
];

const isActive = (path: string, i: NavItem) =>
  i.exact ? path === i.href : path === i.href || path.startsWith(`${i.href}/`);

function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
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
            {i.label}
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
  const navigate = useNavigate();
  const openSettings = useOpenUserSettings();
  const me = useQuery({ queryKey: keys.me(), queryFn: getMe, staleTime: 5 * 60_000 });
  const name = me.data?.global_name ?? me.data?.display_name ?? me.data?.username ?? 'You';
  // My uploads / Upload / Requests live in the submissions flow, which today is under /library.
  const items: MenuEntry[] = [
    { type: 'label', label: name },
    { label: 'Profile', onSelect: () => openSettings() },
    { label: 'My uploads', onSelect: () => void navigate({ to: '/library/submissions' }) },
    { label: 'Upload', onSelect: () => void navigate({ to: '/library/upload' }) },
    { label: 'Requests', onSelect: () => void navigate({ to: '/library/requests' }) },
    { label: 'Settings', onSelect: () => openSettings() },
    { type: 'separator' },
    { label: 'Back to Lounge', onSelect: () => void navigate({ to: '/channels' }) },
  ];
  return (
    <DropdownMenu
      label="Account"
      placement="bottom-end"
      items={items}
      trigger={
        <button type="button" className="cin-avatar-btn" aria-label="Account menu">
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
  return (
    <div className="cin-header">
      {onMenu && (
        <button
          type="button"
          className="cin-icon-btn"
          aria-label="Open cinema menu"
          onClick={onMenu}
        >
          <Icon name="menu" size={20} />
        </button>
      )}
      <a href="/cinema" className="cin-logo" aria-label="Side Room Cinema, Discover">
        <DoorLogo size={32} />
        <span className="cin-logo-text">
          <span className="cin-logo-name">SIDE ROOM</span>
          <span className="cin-logo-sub" aria-hidden="true">
            C I N E M A
          </span>
        </span>
      </a>
      {!onMenu && (
        <nav aria-label="Cinema" className="cin-nav">
          <NavLinks />
        </nav>
      )}
      <div className="cin-header-tools">
        <button
          type="button"
          className="cin-icon-btn"
          aria-label="Search the cinema"
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
  return (
    <footer className="cin-footer">
      <span className="cin-footer-brand">
        Side Room <em>Cinema</em>
      </span>
      <span className="cin-footer-tag">A little further from the ordinary.</span>
      <a href="/house-notes" className="cin-footer-link">
        About this library
      </a>
    </footer>
  );
}

/** The page frame around every Cinema page except the player. */
export function CinemaShell({ children }: { children?: ReactNode }) {
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
        <Drawer open={drawer} onClose={() => setDrawer(false)} title="The cinema" side="left">
          <nav aria-label="Cinema menu">
            <NavLinks onNavigate={() => setDrawer(false)} />
          </nav>
        </Drawer>
      )}
      <SearchOverlay open={search} onClose={() => setSearch(false)} />
    </div>
  );
}
