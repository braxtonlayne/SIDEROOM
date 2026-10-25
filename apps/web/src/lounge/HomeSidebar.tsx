import { Link, useNavigate } from '@tanstack/react-router';
import {
  Avatar,
  AvatarStack,
  ContextMenu,
  CounterBadge,
  Icon,
  VirtualList,
  type MenuEntry,
} from '@sideroom/ui';
import { useT } from '../i18n';
import { useDeveloperMode } from '../settings/user/prefs';
import { useOpenUserSettings } from '../settings/user/nav';
import { useStore } from '../store';
import { dmTitle, HOME } from './channels';
import {
  usePresenceMap,
  useDms,
  useLoungeActions,
  useRequestCounts,
  useUsers,
  type DmChannel,
} from './data';
import { useLoungeRoute } from './nav';
import { closeDm } from './shell/api';
import { startDmCall } from './shell/dmCall';
import { isMuted, MUTE_DURATIONS, useNotifSettings } from './shell/notifSettings';
import { useSupportEnabled } from './shell/useSupportEnabled';
import { useUserMenuEntries } from './shell/userMenu';
import { useLoungeUi } from './store';
import { ServerTagChip } from './components/ServerTagChip';

function DmRow({ dm, selected }: { dm: DmChannel; selected: boolean }) {
  const t = useT();
  const developerMode = useDeveloperMode();
  const users = useUsers();
  const presence = usePresenceMap();
  const actions = useLoungeActions();
  const navigate = useNavigate();
  const notif = useNotifSettings((s) => s.byGuild[HOME]?.channels[dm.id]);
  const patchChannel = useNotifSettings((s) => s.patchChannel);
  const first = users[dm.recipientIds[0] ?? ''];
  const group = dm.recipientIds.length > 1;
  const title = dmTitle(dm, (id) => users[id]?.displayName ?? t('lounge.homeSidebar.unknown'));
  const p = first ? presence[first.id] : undefined;
  const sub = group
    ? t('lounge.homeSidebar.members', { count: dm.recipientIds.length + 1 })
    : p?.activity?.text;
  const muted = isMuted(notif);
  const userEntries = useUserMenuEntries(first ? { id: first.id, name: title } : undefined);

  const close = () => {
    closeDm(dm.id)
      .then(() => {
        useStore.getState().applyDispatch('CHANNEL_DELETE', { id: dm.id });
        if (selected) void navigate({ to: '/channels/$guildId', params: { guildId: HOME } });
      })
      .catch(() => undefined);
  };

  const menu: MenuEntry[] = [
    {
      label: t('lounge.homeSidebar.profile'),
      disabled: group,
      onSelect: () =>
        void navigate({
          to: '/channels/$guildId/$channelId',
          params: { guildId: HOME, channelId: dm.id },
        }),
    },
    {
      label: t('lounge.homeSidebar.call'),
      onSelect: () => startDmCall(dm.id, title, false),
    },
    ...(group || !userEntries.note
      ? [{ label: t('lounge.homeSidebar.addNote'), disabled: true } as MenuEntry]
      : [userEntries.note]),
    { type: 'separator' },
    {
      label: group ? t('lounge.homeSidebar.leaveGroup') : t('lounge.homeSidebar.closeDm'),
      onSelect: close,
    },
    {
      label: t('lounge.homeSidebar.markRead'),
      disabled: !dm.unread,
      onSelect: () => actions.markRead(dm.id),
    },
    muted
      ? {
          label: t('lounge.homeSidebar.unmute'),
          onSelect: () =>
            void patchChannel(HOME, dm.id, { mute: { muted: false } }).catch(() => undefined),
        }
      : {
          type: 'submenu',
          label: t('lounge.homeSidebar.mute'),
          items: MUTE_DURATIONS.map((d) => ({
            label: d.label,
            onSelect: () =>
              void patchChannel(HOME, dm.id, { mute: { muted: true, seconds: d.seconds } }).catch(
                () => undefined,
              ),
          })),
        },
    ...(!group && first
      ? ([
          { type: 'separator' },
          ...(userEntries.invite ? [userEntries.invite] : []),
          ...userEntries.relationship,
          ...(developerMode
            ? [
                {
                  label: t('lounge.homeSidebar.copyUserId'),
                  onSelect: () => void navigator.clipboard?.writeText(first.id),
                },
              ]
            : []),
        ] as MenuEntry[])
      : []),
  ];

  return (
    <ContextMenu items={menu} label={t('lounge.homeSidebar.conversationMenu', { name: title })}>
      <div className="lng-dm-wrap" data-muted={muted || undefined}>
        <Link
          to="/channels/$guildId/$channelId"
          params={{ guildId: HOME, channelId: dm.id }}
          className="lng-dm"
          data-unread={dm.unread || undefined}
          data-selected={selected || undefined}
          aria-current={selected ? 'page' : undefined}
          aria-label={`${title}${dm.unread ? t('lounge.homeSidebar.unreadSuffix') : ''}${muted ? t('lounge.homeSidebar.mutedSuffix') : ''}`}
        >
          {group ? (
            <AvatarStack
              aria-hidden="true"
              size={24}
              max={2}
              items={dm.recipientIds.map((id) => ({
                id,
                name: users[id]?.displayName ?? '?',
                src: users[id]?.avatarUrl ?? null,
              }))}
            />
          ) : (
            <Avatar name={title} size={32} {...(p ? { status: p.status } : {})} />
          )}
          <span className="lng-dm-text">
            <span className="lng-dm-name">
              {title}
              {!group && first && <ServerTagChip user={first} />}
            </span>
            {sub && <span className="lng-dm-sub">{sub}</span>}
          </span>
          <CounterBadge
            count={muted ? 0 : dm.mentionCount}
            noun={t('lounge.homeSidebar.mentionsNoun')}
            className="lng-mention-badge lng-mention-inline"
          />
        </Link>
        <button
          type="button"
          className="lng-icon-btn lng-dm-close"
          aria-label={
            group
              ? t('lounge.homeSidebar.leaveGroupLabel', { name: title })
              : t('lounge.homeSidebar.closeDmLabel', { name: title })
          }
          onClick={close}
        >
          <Icon name="close" size={16} />
        </button>
      </div>
    </ContextMenu>
  );
}

export function HomeSidebar({ onOpenSwitcher }: { onOpenSwitcher: () => void }) {
  const t = useT();
  const dms = useDms();
  const counts = useRequestCounts();
  const { channelId, isHome } = useLoungeRoute();
  const navigate = useNavigate();
  const supportEnabled = useSupportEnabled();
  const openUserSettings = useOpenUserSettings();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const homeView = useLoungeUi((s) => s.homeView);
  const setHomeView = useLoungeUi((s) => s.setHomeView);
  const sorted = [...dms].sort((a, b) => b.lastMessageAt.localeCompare(a.lastMessageAt));
  const onFriends = isHome && !channelId && homeView === 'friends';
  const onRequests = isHome && !channelId && homeView === 'requests';
  const goHome = (view: 'friends' | 'requests') => {
    setHomeView(view);
    void navigate({ to: '/channels/$guildId', params: { guildId: HOME } });
  };
  return (
    <div
      className="lng-sidebar"
      data-testid="home-sidebar"
      role="region"
      aria-label={t('a11y.landmark.homeSidebar')}
    >
      <div className="lng-sidebar-top">
        <button type="button" className="lng-find" onClick={onOpenSwitcher}>
          {t('lounge.homeSidebar.find')}
        </button>
      </div>
      <nav className="lng-home-nav" aria-label={t('lounge.homeSidebar.home')}>
        <Link
          to="/channels/$guildId"
          params={{ guildId: HOME }}
          className="lng-nav-item"
          data-selected={onFriends || undefined}
          aria-current={onFriends ? 'page' : undefined}
          onClick={() => setHomeView('friends')}
        >
          <Icon name="users" size={20} />
          <span>{t('lounge.homeSidebar.friends')}</span>
          <CounterBadge
            count={counts.friends}
            noun={t('lounge.homeSidebar.friendRequestsNoun')}
            className="lng-mention-badge lng-mention-inline"
          />
        </Link>
        <button
          type="button"
          className="lng-nav-item"
          data-selected={onRequests || undefined}
          aria-current={onRequests ? 'page' : undefined}
          onClick={() => goHome('requests')}
        >
          <Icon name="user" size={20} />
          <span>{t('lounge.homeSidebar.messageRequests')}</span>
          <CounterBadge
            count={counts.messages}
            noun={t('lounge.homeSidebar.messageRequestsNoun')}
            className="lng-mention-badge lng-mention-inline"
          />
        </button>
        {supportEnabled && (
          <button
            type="button"
            className="lng-nav-item"
            onClick={() => openUserSettings('support')}
          >
            <Icon name="heart" size={20} />
            <span>{t('lounge.homeSidebar.support')}</span>
          </button>
        )}
      </nav>
      <div className="lng-dm-head">
        <h2>{t('lounge.homeSidebar.directMessages')}</h2>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.homeSidebar.createDm')}
          onClick={() => openDialog({ kind: 'createDm' })}
        >
          <Icon name="plus" size={16} />
        </button>
      </div>
      <VirtualList
        className="lng-dm-list"
        aria-label={t('lounge.homeSidebar.dmList')}
        count={sorted.length}
        estimateSize={44}
        getItemKey={(i) => sorted[i]?.id ?? i}
        renderItem={(i) => {
          const dm = sorted[i];
          return dm ? <DmRow dm={dm} selected={dm.id === channelId} /> : null;
        }}
        empty={<p className="lng-muted lng-pad">{t('lounge.homeSidebar.noConversations')}</p>}
      />
    </div>
  );
}
