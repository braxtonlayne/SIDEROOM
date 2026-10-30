// Library section chrome (LIB-UI-001..005): section bar, sidebar, top search row with the bell and avatar menu,
// and the mobile layout (sidebar becomes a drawer, bottom navigation).
import { exitToLogin } from '../lib/enterApp';
import { SkipLink } from '../components/SkipLink';
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
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../api/client';
import { useT, type MessageKey } from '../i18n';
import { getMe } from '../settings/user/api';
import { useOpenUserSettings } from '../settings/user/nav';
import { getFollows, getUpdates, keys } from './api';
import { getNotificationCount, markNotificationsSeen } from './p13api';
import { useLibrarySettings, useSetHideRestricted } from './prefs';
import { SearchBox } from './SearchBox';
import { updateLine } from './ui';

/** `SIDE ROOM / The lounge · The cinema · The library` with the library active (LIB-UI-001). */
export function SectionBar() {
  const t = useT();
  return (
    <header className="lib-sectionbar">
      <span className="lib-sectionbar-brand">{t('library.shell.brand')}</span>
      <span aria-hidden="true" className="lib-sectionbar-slash">
        /
      </span>
      <nav aria-label={t('library.shell.sectionsNav')} className="lib-sectionbar-nav">
        <Link to="/channels" className="lib-sectionbar-link">
          {t('library.shell.lounge')}
        </Link>
        <Link to="/cinema" className="lib-sectionbar-link">
          {t('library.shell.cinema')}
        </Link>
        <Link to="/library" className="lib-sectionbar-link" aria-current="page">
          {t('library.shell.library')}
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
    | '/library/shelves'
    | '/library/upload';
  labelKey: MessageKey;
  icon: 'compass' | 'grid' | 'clock' | 'bookmark' | 'list' | 'upload' | 'edit' | 'heart';
  exact?: boolean;
}

const DISCOVER: NavItem[] = [
  { to: '/library', labelKey: 'library.shell.nav.home', icon: 'compass', exact: true },
  { to: '/library/browse', labelKey: 'library.shell.nav.browse', icon: 'grid' },
  { to: '/library/updates', labelKey: 'library.shell.nav.latestUpdates', icon: 'clock' },
  { to: '/library/requests', labelKey: 'library.shell.nav.requests', icon: 'heart' },
  { to: '/library/shelves', labelKey: 'shelves.nav', icon: 'bookmark' },
];
const YOUR_SPACE: NavItem[] = [
  { to: '/library/me', labelKey: 'library.shell.nav.myLibrary', icon: 'bookmark' },
  { to: '/library/history', labelKey: 'library.shell.nav.readingHistory', icon: 'clock' },
  { to: '/library/lists', labelKey: 'library.shell.nav.readingLists', icon: 'list' },
  { to: '/library/submissions', labelKey: 'library.shell.nav.submissions', icon: 'edit' },
  { to: '/library/upload', labelKey: 'library.shell.nav.upload', icon: 'upload' },
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
  const t = useT();
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
            <span>{t(i.labelKey)}</span>
            {i.to === '/library/me' && count !== undefined && count > 0 && (
              <span
                className="lib-nav-count"
                aria-label={t('library.shell.followedCount', { count })}
              >
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
  const t = useT();
  const openSettings = useOpenUserSettings();
  const follows = useQuery({
    queryKey: keys.follows(),
    queryFn: () => getFollows({ limit: 1 }),
    staleTime: 60_000,
  });
  return (
    <nav className="lib-sidebar" aria-label={t('library.shell.sidebarNav')}>
      <div className="lib-sidebar-head">
        <span className="lib-logo-box" aria-hidden="true">
          <Icon name="book" size={20} />
        </span>
        <span className="lib-sidebar-title">{t('library.shell.library')}</span>
      </div>
      <p className="lib-nav-heading" id="lib-nav-discover">
        {t('library.shell.discover')}
      </p>
      <NavLinks items={DISCOVER} onNavigate={onNavigate} />
      <p className="lib-nav-heading" id="lib-nav-space">
        {t('library.shell.yourSpace')}
      </p>
      <NavLinks items={YOUR_SPACE} count={follows.data?.total} onNavigate={onNavigate} />
      <div className="lib-sidebar-foot">
        <button type="button" className="lib-nav-link" onClick={onHelp}>
          <Icon name="info" size={20} />
          <span>{t('library.shell.helpAndShortcuts')}</span>
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
          <span>{t('library.shell.settings')}</span>
        </button>
        <p className="lib-tagline">{t('library.shell.tagline')}</p>
      </div>
    </nav>
  );
}

function Bell() {
  const t = useT();
  const navigate = useNavigate();
  const qc = useQueryClient();
  // New chapters of followed series (bell on) since the member last opened the bell (LIB-SOC-007).
  const count = useQuery({
    queryKey: ['library', 'bell-count'],
    queryFn: getNotificationCount,
    refetchInterval: 120_000,
    staleTime: 30_000,
    retry: false,
  });
  const unread = count.data?.unread ?? 0;
  const seen = () => {
    if (unread === 0) return;
    qc.setQueryData(['library', 'bell-count'], { unread: 0 });
    void markNotificationsSeen().catch(() => undefined);
  };
  const updates = useQuery({
    queryKey: keys.updates(true),
    queryFn: () => getUpdates({ following: true, limit: 6 }),
    staleTime: 60_000,
  });
  const items: MenuEntry[] = [{ type: 'label', label: t('library.shell.bell.fromFollowed') }];
  const list = updates.data?.items ?? [];
  if (list.length === 0)
    items.push({
      label: updates.isPending ? t('library.shell.bell.loading') : t('library.shell.bell.empty'),
      disabled: true,
    });
  for (const u of list)
    items.push({
      label: t('library.shell.bell.item', { title: u.series_title, line: updateLine(u) }),
      onSelect: () =>
        void navigate({
          to: '/library/r/$chapterId/{-$page}',
          params: { chapterId: u.chapter_id },
        }),
    });
  items.push(
    { type: 'separator' },
    {
      label: t('library.shell.bell.allUpdates'),
      onSelect: () => void navigate({ to: '/library/updates' }),
    },
  );
  return (
    <DropdownMenu
      label={t('library.shell.bell.label')}
      placement="bottom-end"
      items={items}
      onOpenChange={(open) => open && seen()}
      trigger={
        <button
          type="button"
          className="lib-icon-btn lib-bell"
          aria-label={
            unread > 0
              ? t('library.shell.bell.labelCount', { n: unread })
              : t('library.shell.bell.label')
          }
        >
          <Icon name="bell" size={20} />
          {unread > 0 && (
            <span className="lib-bell-badge" aria-hidden="true">
              {unread >= 99 ? '99+' : unread}
            </span>
          )}
        </button>
      }
    />
  );
}

function AvatarMenu() {
  const t = useT();
  const navigate = useNavigate();
  const openSettings = useOpenUserSettings();
  const { hideRestricted } = useLibrarySettings();
  const setHide = useSetHideRestricted();
  const me = useQuery({ queryKey: ['library', 'me'], queryFn: getMe, staleTime: 5 * 60_000 });
  const name =
    me.data?.global_name ??
    me.data?.display_name ??
    me.data?.username ??
    t('library.shell.avatar.you');
  const items: MenuEntry[] = [
    { type: 'label', label: name },
    {
      type: 'checkbox',
      label: t('library.shell.avatar.hideRestricted'),
      checked: hideRestricted,
      onCheckedChange: (v) => void setHide(v),
      keepOpen: true,
    },
    { label: t('library.shell.avatar.userSettings'), onSelect: () => openSettings() },
    { type: 'separator' },
    {
      label: t('library.shell.avatar.backToLounge'),
      onSelect: () => void navigate({ to: '/channels' }),
    },
  ];
  return (
    <DropdownMenu
      label={t('library.shell.avatar.label')}
      placement="bottom-end"
      items={items}
      trigger={
        <button
          type="button"
          className="lib-avatar-btn"
          aria-label={t('library.shell.avatar.menuLabel')}
        >
          <Avatar name={name} size={32} />
        </button>
      }
    />
  );
}

const SHORTCUTS: { keys: string[]; text: MessageKey }[] = [
  { keys: ['/'], text: 'library.shell.shortcut.search' },
  { keys: ['←', '→'], text: 'library.shell.shortcut.turnPage' },
  { keys: ['Space'], text: 'library.shell.shortcut.nextPage' },
  { keys: ['Home', 'End'], text: 'library.shell.shortcut.firstLast' },
  { keys: ['D'], text: 'library.shell.shortcut.doublePage' },
  { keys: ['M'], text: 'library.shell.shortcut.settings' },
  { keys: ['B'], text: 'library.shell.shortcut.bookmark' },
  { keys: ['F'], text: 'library.shell.shortcut.fullScreen' },
  { keys: ['+', '-', '0'], text: 'library.shell.shortcut.zoom' },
];

function HelpSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  return (
    <Modal open={open} onClose={onClose} title={t('library.shell.helpAndShortcuts')} size="md">
      <dl className="lib-shortcuts">
        {SHORTCUTS.map((s) => (
          <div key={s.text} className="lib-shortcut-row">
            <dt>{t(s.text)}</dt>
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
    exitToLogin(here);
  }, [unauthorized, navigate, here]);
  return unauthorized;
}

const BOTTOM: NavItem[] = [DISCOVER[0]!, DISCOVER[1]!, DISCOVER[2]!, YOUR_SPACE[0]!];

/** The page frame around every non-reader Library page. */
export function LibraryShell({ children }: { children?: ReactNode }) {
  const t = useT();
  const mobile = useMediaQuery(MOBILE_QUERY);
  const [drawer, setDrawer] = useState(false);
  const [help, setHelp] = useState(false);
  const searchWrap = useRef<HTMLDivElement>(null);

  // "/" jumps to search, as on most reading sites.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target;
      const typing =
        target instanceof HTMLElement &&
        (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
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
      <SkipLink />
      <SectionBar />
      <div className="lib-body">
        {!mobile && (
          <aside className="lib-side">
            <Sidebar onHelp={() => setHelp(true)} />
          </aside>
        )}
        <div className="lib-main">
          <div className="lib-topbar" role="region" aria-label={t('a11y.landmark.libraryToolbar')}>
            {mobile && (
              <button
                type="button"
                className="lib-icon-btn"
                aria-label={t('library.shell.openMenu')}
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
        <nav className="lib-bottomnav" aria-label={t('library.shell.quickLinks')}>
          {BOTTOM.map((i) => (
            <Link
              key={i.to}
              to={i.to}
              className="lib-bottomnav-link"
              activeOptions={{ exact: i.exact ?? false }}
              activeProps={{ 'aria-current': 'page' }}
            >
              <Icon name={i.icon} size={20} />
              <span>
                {i.labelKey === 'library.shell.nav.latestUpdates'
                  ? t('library.shell.nav.updates')
                  : t(i.labelKey)}
              </span>
            </Link>
          ))}
        </nav>
      )}
      {mobile && (
        <Drawer
          open={drawer}
          onClose={() => setDrawer(false)}
          title={t('library.shell.library')}
          side="left"
        >
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
