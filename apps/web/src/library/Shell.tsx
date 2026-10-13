// Library section chrome (LIB-UI-001..005): section bar, sidebar, top search row with the bell and avatar menu,
// and the mobile layout (sidebar becomes a drawer, bottom navigation).
import {
  Avatar,
  Drawer,
  DropdownMenu,
  Icon,
  KbdCombo,
  Modal,
  MOBILE_QUERY,
  useMediaQuery,
  type MenuEntry,
} from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { Link, Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../api/client';
import { getMe } from '../settings/user/api';
import { useOpenUserSettings } from '../settings/user/nav';
import { getFollows, getUpdates, keys } from './api';
import { useLibrarySettings, useSetHideRestricted } from './prefs';
import { SearchBox } from './SearchBox';
import { updateLine } from './ui';

/** `SIDE ROOM / The lounge · The cinema · The library` with the library active (LIB-UI-001). */
export function SectionBar() {
  return (
    <header className="lib-sectionbar">
      <span className="lib-sectionbar-brand">SIDE ROOM</span>
      <span aria-hidden="true" className="lib-sectionbar-slash">
        /
      </span>
      <nav aria-label="Sections" className="lib-sectionbar-nav">
        <Link to="/channels" className="lib-sectionbar-link">
          The lounge
        </Link>
        <Link to="/cinema" className="lib-sectionbar-link">
          The cinema
        </Link>
        <Link to="/library" className="lib-sectionbar-link" aria-current="page">
          The library
        </Link>
      </nav>
    </header>
  );
}

interface NavItem {
  to:
    | '/library'
    | '/library/browse'
    | '/library/updates'
    | '/library/requests'
    | '/library/me'
    | '/library/history'
    | '/library/lists'
    | '/library/submissions'
    | '/library/upload';
  label: string;
  icon: 'compass' | 'grid' | 'clock' | 'bookmark' | 'list' | 'upload' | 'edit' | 'heart';
  exact?: boolean;
}

const DISCOVER: NavItem[] = [
  { to: '/library', label: 'Home', icon: 'compass', exact: true },
  { to: '/library/browse', label: 'Browse', icon: 'grid' },
  { to: '/library/updates', label: 'Latest updates', icon: 'clock' },
  { to: '/library/requests', label: 'Requests', icon: 'heart' },
];
const YOUR_SPACE: NavItem[] = [
  { to: '/library/me', label: 'My library', icon: 'bookmark' },
  { to: '/library/history', label: 'Reading history', icon: 'clock' },
  { to: '/library/lists', label: 'Reading lists', icon: 'list' },
  { to: '/library/submissions', label: 'Submissions', icon: 'edit' },
  { to: '/library/upload', label: 'Upload', icon: 'upload' },
];

function NavLinks({
  items,
  count,
  onNavigate,
}: {
  items: NavItem[];
  count?: number;
  onNavigate?: () => void;
}) {
  return (
    <ul className="lib-nav-list">
      {items.map((i) => (
        <li key={i.to}>
          <Link
            to={i.to}
            className="lib-nav-link"
            activeOptions={{ exact: i.exact ?? false }}
            activeProps={{ 'aria-current': 'page' }}
            onClick={onNavigate}
          >
            <Icon name={i.icon} size={20} />
            <span>{i.label}</span>
            {i.to === '/library/me' && count !== undefined && count > 0 && (
              <span className="lib-nav-count" aria-label={`${count} followed`}>
                {count}
              </span>
            )}
          </Link>
        </li>
      ))}
    </ul>
  );
}

function Sidebar({ onNavigate, onHelp }: { onNavigate?: () => void; onHelp: () => void }) {
  const openSettings = useOpenUserSettings();
  const follows = useQuery({
    queryKey: keys.follows(),
    queryFn: () => getFollows({ limit: 1 }),
    staleTime: 60_000,
  });
  return (
    <nav className="lib-sidebar" aria-label="Library">
      <div className="lib-sidebar-head">
        <span className="lib-logo-box" aria-hidden="true">
          <Icon name="book" size={20} />
        </span>
        <span className="lib-sidebar-title">The library</span>
      </div>
      <p className="lib-nav-heading" id="lib-nav-discover">
        Discover
      </p>
      <NavLinks items={DISCOVER} onNavigate={onNavigate} />
      <p className="lib-nav-heading" id="lib-nav-space">
        Your space
      </p>
      <NavLinks items={YOUR_SPACE} count={follows.data?.total} onNavigate={onNavigate} />
      <div className="lib-sidebar-foot">
        <button type="button" className="lib-nav-link" onClick={onHelp}>
          <Icon name="info" size={20} />
          <span>Help &amp; shortcuts</span>
        </button>
        <button
          type="button"
          className="lib-nav-link"
          onClick={() => {
            onNavigate?.();
            openSettings();
          }}
        >
          <Icon name="settings" size={20} />
          <span>Settings</span>
        </button>
        <p className="lib-tagline">Manga, at your pace.</p>
      </div>
    </nav>
  );
}

function Bell() {
  const navigate = useNavigate();
  const updates = useQuery({
    queryKey: keys.updates(true),
    queryFn: () => getUpdates({ following: true, limit: 6 }),
    staleTime: 60_000,
  });
  const items: MenuEntry[] = [{ type: 'label', label: 'From series you follow' }];
  const list = updates.data?.items ?? [];
  if (list.length === 0)
    items.push({ label: updates.isPending ? 'Loading…' : 'Nothing new yet', disabled: true });
  for (const u of list)
    items.push({
      label: `${u.series_title}: ${updateLine(u)}`,
      onSelect: () =>
        void navigate({
          to: '/library/r/$chapterId/{-$page}',
          params: { chapterId: u.chapter_id },
        }),
    });
  items.push(
    { type: 'separator' },
    { label: 'All updates', onSelect: () => void navigate({ to: '/library/updates' }) },
  );
  return (
    <DropdownMenu
      label="Notifications"
      placement="bottom-end"
      items={items}
      trigger={
        <button type="button" className="lib-icon-btn" aria-label="Notifications">
          <Icon name="bell" size={20} />
        </button>
      }
    />
  );
}

function AvatarMenu() {
  const navigate = useNavigate();
  const openSettings = useOpenUserSettings();
  const { hideRestricted } = useLibrarySettings();
  const setHide = useSetHideRestricted();
  const me = useQuery({ queryKey: ['library', 'me'], queryFn: getMe, staleTime: 5 * 60_000 });
  const name = me.data?.global_name ?? me.data?.display_name ?? me.data?.username ?? 'You';
  const items: MenuEntry[] = [
    { type: 'label', label: name },
    {
      type: 'checkbox',
      label: 'Hide 18+ titles',
      checked: hideRestricted,
      onCheckedChange: (v) => void setHide(v),
      keepOpen: true,
    },
    { label: 'User settings', onSelect: () => openSettings() },
    { type: 'separator' },
    { label: 'Back to the lounge', onSelect: () => void navigate({ to: '/channels' }) },
  ];
  return (
    <DropdownMenu
      label="Account"
      placement="bottom-end"
      items={items}
      trigger={
        <button type="button" className="lib-avatar-btn" aria-label="Account menu">
          <Avatar name={name} size={32} />
        </button>
      }
    />
  );
}

const SHORTCUTS: { keys: string[]; text: string }[] = [
  { keys: ['/'], text: 'Search the library' },
  { keys: ['←', '→'], text: 'Reader: turn the page (follows the reading direction)' },
  { keys: ['Space'], text: 'Reader: next page (Shift: previous)' },
  { keys: ['Home', 'End'], text: 'Reader: first or last page' },
  { keys: ['D'], text: 'Reader: toggle double-page mode' },
  { keys: ['M'], text: 'Reader: reader settings' },
  { keys: ['B'], text: 'Reader: bookmark this page' },
  { keys: ['F'], text: 'Reader: full screen' },
  { keys: ['+', '-', '0'], text: 'Reader: zoom in, out, reset' },
];

function HelpSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  return (
    <Modal open={open} onClose={onClose} title="Help & shortcuts" size="md">
      <dl className="lib-shortcuts">
        {SHORTCUTS.map((s) => (
          <div key={s.text} className="lib-shortcut-row">
            <dt>{s.text}</dt>
            <dd>
              <KbdCombo keys={s.keys} />
            </dd>
          </div>
        ))}
      </dl>
    </Modal>
  );
}

export function useRequireSession() {
  const navigate = useNavigate();
  const here = useRouterState({ select: (s) => s.location.href });
  const sent = useRef(false);
  const me = useQuery({
    queryKey: ['library', 'me'],
    queryFn: getMe,
    staleTime: 5 * 60_000,
    retry: false,
  });
  const unauthorized = me.error instanceof ApiError && me.error.status === 401;
  useEffect(() => {
    if (!unauthorized || sent.current) return;
    sent.current = true; // the href changes once we leave; never chain redirects
    void navigate({ to: '/login', search: { next: here } });
  }, [unauthorized, navigate, here]);
  return unauthorized;
}

const BOTTOM: NavItem[] = [DISCOVER[0]!, DISCOVER[1]!, DISCOVER[2]!, YOUR_SPACE[0]!];

/** The page frame around every non-reader Library page. */
export function LibraryShell({ children }: { children?: ReactNode }) {
  const mobile = useMediaQuery(MOBILE_QUERY);
  const [drawer, setDrawer] = useState(false);
  const [help, setHelp] = useState(false);
  const searchWrap = useRef<HTMLDivElement>(null);

  // "/" jumps to search, as on most reading sites.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target;
      const typing =
        t instanceof HTMLElement &&
        (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if (e.key === '/' && !typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        searchWrap.current?.querySelector('input')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <div className="lib-frame" data-mobile={mobile || undefined}>
      <SectionBar />
      <div className="lib-body">
        {!mobile && (
          <aside className="lib-side">
            <Sidebar onHelp={() => setHelp(true)} />
          </aside>
        )}
        <div className="lib-main">
          <div className="lib-topbar">
            {mobile && (
              <button
                type="button"
                className="lib-icon-btn"
                aria-label="Open library menu"
                onClick={() => setDrawer(true)}
              >
                <Icon name="menu" size={20} />
              </button>
            )}
            <div ref={searchWrap} className="lib-topbar-search">
              <SearchBox />
            </div>
            <Bell />
            <AvatarMenu />
          </div>
          <main className="lib-content" id="lib-content">
            {children ?? <Outlet />}
          </main>
        </div>
      </div>
      {mobile && (
        <nav className="lib-bottomnav" aria-label="Library quick links">
          {BOTTOM.map((i) => (
            <Link
              key={i.to}
              to={i.to}
              className="lib-bottomnav-link"
              activeOptions={{ exact: i.exact ?? false }}
              activeProps={{ 'aria-current': 'page' }}
            >
              <Icon name={i.icon} size={20} />
              <span>{i.label === 'Latest updates' ? 'Updates' : i.label}</span>
            </Link>
          ))}
        </nav>
      )}
      {mobile && (
        <Drawer open={drawer} onClose={() => setDrawer(false)} title="The library" side="left">
          <Sidebar
            onNavigate={() => setDrawer(false)}
            onHelp={() => {
              setDrawer(false);
              setHelp(true);
            }}
          />
        </Drawer>
      )}
      <HelpSheet open={help} onClose={() => setHelp(false)} />
    </div>
  );
}
