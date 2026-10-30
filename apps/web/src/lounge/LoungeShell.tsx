import { ThemeProvider, ToastProvider, Icon } from '@sideroom/ui';
import { SkipLink } from '../components/SkipLink';
import { useNavigate } from '@tanstack/react-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { TouchEvent } from 'react';
import { useT } from '../i18n';
import { useAppBadge } from '../pwa/useAppBadge';
import { useDesktopNotify } from './shell/desktopNotify';
import { DmToasts } from './shell/DmToasts';
import { JoinGate } from './shell/JoinGate';
import { Welcome } from './shell/Welcome';
import { useDesktopAlerts } from '../pwa/useDesktopAlerts';
import { useVisualViewport } from './useVisualViewport';
import { ChannelSidebar } from './ChannelSidebar';
import { ActivityDock } from '../activities/ActivityDock';
import { ChannelView } from './ChannelView';
import { SectionBar, TitleStrip } from './Chrome';
import { MOBILE_QUERY, useMediaQuery } from '@sideroom/ui';
import { ResizeHandle } from '@sideroom/ui';
import { ComingSoon } from '@sideroom/ui';
import { firstTextChannel, HOME, navigableChannels, step } from './channels';
import { useOpenUserSettings } from '../settings/user/nav';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { useServerAppearance } from '../packages/serverAppearance';
import { useStoreThemes } from '../packages/themes';
import { loadMarkdownEngine } from './markdown';
import { loadPermEngine } from './permEngine';
import {
  useAllChannels,
  useChannels,
  useDms,
  useGuild,
  useLoungeActions,
  useRail,
  useUsers,
} from './data';
import { GuildRail } from './GuildRail';
import { HomeSidebar } from './HomeSidebar';
import { LoungeDialogs } from './LoungeDialogs';
import { MemberList } from './MemberList';
import { useLoungeGoto, useLoungeRoute } from './nav';
import { DmProfilePanel, HomeFriends, PickChannel } from './Panels';
import { openDrawer } from '../tools/drawer/openState';
import { ViewAsBanner } from '../settings/server/viewAsMode';
import { ConnectionBanner, NoticesBar } from './shell/Banners';
import { useAgeCleared, useAgeGate } from './AgeGate';
import { PopoutHost } from './profile/ProfilePopout';
import { ReminderTicker } from './shell/ReminderTicker';
import { InteractionModalHost } from './apps/InteractionModal';
import { ThreadPane } from './threads/ThreadPane';
import { useThreadRoute } from './threads/useThreadRoute';
import { useShellBoot } from './shell/useShellBoot';
import { ShortcutSheet } from './ShortcutSheet';
import { isDrawerShortcut, isPlayerShortcut, matchCustomShortcut } from './shortcuts';
import { usePrefsStore } from '../settings/user/prefs';
import { RIGHT_PANEL_MAX, RIGHT_PANEL_MIN, useLoungeUi } from './store';
import { UserPanel } from './UserPanel';
import { EventAlerts } from './events/EventAlerts';
import './lounge.css';

type Pane = 'list' | 'chat' | 'members';

function isEditable(t: EventTarget | null) {
  return (
    t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))
  );
}

/** Keyboard shortcut dispatcher; definitions live in shortcuts.ts. */
function useLoungeShortcuts(h: {
  toggleSheet: () => void;
  openSettings: () => void;
}) {
  const { guildId, channelId } = useLoungeRoute();
  const channelsByGuild = useAllChannels();
  const rail = useRail();
  const goto = useLoungeGoto();
  const actions = useLoungeActions();
  const latest = useRef({ guildId, channelId, channelsByGuild, rail, goto, actions, h });
  latest.current = { guildId, channelId, channelsByGuild, rail, goto, actions, h };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const id = matchCustomShortcut(e, usePrefsStore.getState().prefs.keybinds);
      if (!id || isPlayerShortcut(id) || isDrawerShortcut(id)) return; // the player host and the drawer gate own their keys
      const c = latest.current;
      if (id === 'markRead' || id === 'focusComposer') {
        // Esc and typing belong to open dialogs, menus and text fields first.
        if (isEditable(e.target) || document.querySelector('[role="dialog"], [role="menu"]'))
          return;
        if (id === 'markRead') {
          if (c.channelId) c.actions.markRead(c.channelId);
        } else {
          // Not prevented: the key is typed into the message box that now has focus.
          document.querySelector<HTMLTextAreaElement>('[data-composer]')?.focus();
        }
        return;
      }
      e.preventDefault();
      switch (id) {
        case 'shortcutSheet':
          c.h.toggleSheet();
          return;
        case 'userSettings':
          c.h.openSettings();
          return;
        case 'prevChannel':
        case 'nextChannel':
        case 'prevUnread':
        case 'nextUnread': {
          if (!c.guildId || c.guildId === HOME) return;
          const list = navigableChannels(c.channelsByGuild[c.guildId] ?? []);
          const unreadOnly = id.endsWith('Unread');
          const pool = unreadOnly
            ? list.filter((x) => (x.unread && !x.muted) || x.id === c.channelId)
            : list;
          const next = step(
            pool,
            pool.findIndex((x) => x.id === c.channelId),
            id.startsWith('next') ? 1 : -1,
          );
          if (next && next.id !== c.channelId) c.goto(c.guildId, next.id);
          return;
        }
        case 'prevGuild':
        case 'nextGuild': {
          const ids = [
            HOME,
            ...c.rail.flatMap((r) => (r.kind === 'guild' ? [r.guildId] : r.guildIds)),
          ];
          const target = step(ids, ids.indexOf(c.guildId ?? HOME), id === 'nextGuild' ? 1 : -1);
          if (!target) return;
          if (target === HOME) c.goto(HOME);
          else c.goto(target, firstTextChannel(c.channelsByGuild[target] ?? [])?.id);
          return;
        }
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);
}

/** Mouse/finger-friendly pane switching on narrow screens. */
function useSwipe(order: Pane[], pane: Pane, setPane: (p: Pane) => void) {
  const start = useRef<{ x: number; y: number } | null>(null);
  return {
    onTouchStart: (e: TouchEvent) => {
      const t = e.touches[0];
      start.current = t ? { x: t.clientX, y: t.clientY } : null;
    },
    onTouchEnd: (e: TouchEvent) => {
      const s = start.current;
      const t = e.changedTouches[0];
      start.current = null;
      if (!s || !t) return;
      const dx = t.clientX - s.x;
      if (Math.abs(dx) < 64 || Math.abs(dx) < Math.abs(t.clientY - s.y) * 1.5) return;
      const i = order.indexOf(pane);
      const next = order[dx > 0 ? i - 1 : i + 1];
      if (next) setPane(next);
    },
  };
}

function Main({ isMobile, onBack }: { isMobile: boolean; onBack: () => void }) {
  const route = useLoungeRoute();
  const { guildId } = route;
  const channels = useChannels(guildId && guildId !== HOME ? guildId : undefined);
  // /channels/:guild/:thread shows the thread's parent with the thread in full view (G-31).
  const resolved = useThreadRoute(
    guildId === HOME ? undefined : guildId,
    route.channelId,
    route.threadId,
    channels.map((c) => c.id),
  );
  const channelId = route.channelId ? resolved.parentId : undefined;
  const threadId = resolved.threadId;
  const navigate = useNavigate();
  const remembered = useLoungeUi((s) => (guildId ? s.lastChannelByGuild[guildId] : undefined));
  // A thread of an age-restricted channel stays closed until the channel's question is answered.
  const gate = useAgeGate(channels.find((c) => c.id === channelId)?.nsfw);
  const restrictedParent = gate.state !== 'open';
  const ageCleared = useAgeCleared();

  // /channels/:guild without a channel -> remembered or first text channel (desktop; mobile keeps the list visible).
  useEffect(() => {
    if (!guildId || guildId === HOME || channelId || isMobile) return;
    // Landing never picks an age-restricted channel the viewer has not been cleared for.
    const open = channels.filter((c) => !c.nsfw || ageCleared);
    const target = open.find((c) => c.id === remembered) ?? firstTextChannel(open);
    if (target)
      void navigate({
        to: '/channels/$guildId/$channelId',
        params: { guildId, channelId: target.id },
        replace: true,
      });
  }, [guildId, channelId, channels, remembered, navigate, isMobile, ageCleared]);

  if (!guildId) return null;
  if (!channelId) return guildId === HOME ? <HomeFriends /> : <PickChannel guildId={guildId} />;
  if (resolved.pending) return null;
  return (
    <>
      <ChannelView
        key={channelId}
        guildId={guildId}
        channelId={channelId}
        onBack={isMobile ? onBack : undefined}
      />
      {threadId && guildId !== HOME && !restrictedParent && (
        <ThreadPane
          key={threadId}
          guildId={guildId}
          parentId={channelId}
          threadId={threadId}
          fullView={resolved.fullView}
        />
      )}
    </>
  );
}

function Shell() {
  const t = useT();
  const { guildId, channelId } = useLoungeRoute();
  const isHome = guildId === HOME || guildId === undefined;
  const guild = useGuild(isHome ? undefined : guildId);
  const dms = useDms();
  const users = useUsers();
  const isMobile = useMediaQuery(MOBILE_QUERY);
  const setLocation = useLoungeUi((s) => s.setLocation);
  const membersOpen = useLoungeUi((s) => s.memberListOpen);
  const width = useLoungeUi((s) => s.rightPanelWidth);
  const setWidth = useLoungeUi((s) => s.setRightPanelWidth);
  const [sheet, setSheet] = useState(false);
  const [pane, setPane] = useState<Pane>(channelId ? 'chat' : 'list');
  const [tab, setTab] = useState<'home' | 'notifications' | 'you'>('home');
  const [coming, setComing] = useState(false);

  useShellBoot();
  useAppBadge();
  useDesktopNotify();
  useDesktopAlerts();
  useVisualViewport();
  const openSettings = useOpenUserSettings();
  const theme = useAppliedTheme();
  const serverLook = useServerAppearance(isHome ? undefined : guildId, theme.allowServerTheming);
  // A Theme Store preview beats the server's look too (D-280): the server theme and accent step aside while it is open.
  const previewing = useStoreThemes((s) => s.preview !== null);
  useLoungeShortcuts({
    toggleSheet: () => setSheet((s) => !s),
    openSettings,
  });

  useEffect(() => {
    if (guildId) setLocation(guildId, channelId);
  }, [guildId, channelId, setLocation]);

  // Start the WASM markdown parser; messages show as plain text until it is ready.
  useEffect(() => {
    loadMarkdownEngine().catch(() => {
      /* stays on plain text; the next mount retries */
    });
  }, []);

  // Start the WASM permission engine; channel permissions use the guild level until it is ready (D-178).
  useEffect(() => {
    loadPermEngine().catch(() => {
      /* stays on guild-level permissions; the next mount retries */
    });
  }, []);

  // Picking a channel on mobile reveals the chat pane.
  useEffect(() => {
    if (channelId) setPane('chat');
  }, [channelId]);

  const goList = useCallback(() => setPane('list'), []);
  const swipe = useSwipe(['list', 'chat', 'members'], pane, setPane);

  const dm = dms.find((d) => d.id === channelId);
  const title = isHome
    ? dm
      ? (dm.name ?? dm.recipientIds.map((id) => users[id]?.displayName ?? '').join(', '))
      : t('lounge.loungeShell.directMessages')
    : (guild?.name ?? '');

  const showRight = isHome ? !!dm && membersOpen : !!channelId && membersOpen;

  return (
    <ThemeProvider
      section="lounge"
      serverAccent={previewing ? null : (serverLook.accent ?? guild?.accent ?? null)}
      allowServerTheming={theme.allowServerTheming}
      userGlobal={theme.userGlobal}
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={previewing ? theme.style : { ...theme.style, ...serverLook.style }}
      {...theme.data}
      className="lng-root"
      data-mobile={isMobile || undefined}
      data-pane={isMobile ? pane : undefined}
    >
      <ToastProvider>
        <SkipLink />
        <DmToasts />
        <Welcome />
        <JoinGate />
        <SectionBar />
        <EventAlerts />
        {!isMobile && <TitleStrip title={title} />}
        <ConnectionBanner />
        <NoticesBar />
        <ViewAsBanner />
        <div className="lng-body" {...(isMobile ? swipe : {})}>
          <div className="lng-left" data-pane="list">
            <GuildRail />
            <div className="lng-left-col">
              {isHome ? (
                <HomeSidebar onOpenSwitcher={() => openDrawer('goto')} />
              ) : (
                <ChannelSidebar guildId={guildId!} />
              )}
              <UserPanel />
            </div>
          </div>
          <main className="lng-main" data-pane="chat" id="lounge-main">
            <Main isMobile={isMobile} onBack={goList} />
          </main>
          {showRight && (
            <>
              {!isMobile && (
                <ResizeHandle
                  value={width}
                  min={RIGHT_PANEL_MIN}
                  max={RIGHT_PANEL_MAX}
                  onChange={setWidth}
                  label={t('lounge.loungeShell.resizeRightPanel')}
                />
              )}
              <aside
                className="lng-right"
                data-pane="members"
                style={isMobile ? undefined : { width }}
                aria-label={
                  isHome ? t('lounge.loungeShell.profile') : t('lounge.loungeShell.memberList')
                }
              >
                {isHome ? (
                  channelId && <DmProfilePanel dmId={channelId} />
                ) : (
                  <MemberList guildId={guildId!} channelId={channelId} />
                )}
              </aside>
            </>
          )}
        </div>
        {isMobile && (
          <nav className="lng-tabbar" aria-label={t('lounge.loungeShell.primary')}>
            <button
              type="button"
              aria-current={tab === 'home' ? 'page' : undefined}
              onClick={() => {
                setTab('home');
                setPane('list');
              }}
            >
              <Icon name="home" size={24} />
              {t('lounge.loungeShell.home')}
            </button>
            <button
              type="button"
              aria-current={tab === 'notifications' ? 'page' : undefined}
              onClick={() => {
                setTab('notifications');
                setComing(true);
              }}
            >
              <Icon name="bell" size={24} />
              {t('lounge.loungeShell.notifications')}
            </button>
            <button
              type="button"
              aria-current={tab === 'you' ? 'page' : undefined}
              onClick={() => {
                setTab('you');
                setPane('list');
              }}
            >
              <Icon name="user" size={24} />
              {t('lounge.loungeShell.you')}
            </button>
          </nav>
        )}
        <ComingSoon
          open={coming}
          onClose={() => setComing(false)}
          title={t('lounge.loungeShell.notifications')}
        >
          <p>{t('lounge.loungeShell.inboxNote')}</p>
        </ComingSoon>
        <LoungeDialogs />
        <ActivityDock />
        <InteractionModalHost />
        <PopoutHost />
        <ReminderTicker />
        <ShortcutSheet open={sheet} onClose={() => setSheet(false)} />
      </ToastProvider>
    </ThemeProvider>
  );
}

export function LoungeShell() {
  return <Shell />;
}
