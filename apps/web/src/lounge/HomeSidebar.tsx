import { Link, useNavigate } from '@tanstack/react-router';
import { Avatar, ContextMenu, CounterBadge, Icon, VirtualList, type MenuEntry } from '@sideroom/ui';
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
import { isMuted, MUTE_DURATIONS, useNotifSettings } from './shell/notifSettings';
import { useUserMenuEntries } from './shell/userMenu';
import { useLoungeUi } from './store';

function DmRow({ dm, selected }: { dm: DmChannel; selected: boolean }) {
  const users = useUsers();
  const presence = usePresenceMap();
  const actions = useLoungeActions();
  const navigate = useNavigate();
  const notif = useNotifSettings((s) => s.byGuild[HOME]?.channels[dm.id]);
  const patchChannel = useNotifSettings((s) => s.patchChannel);
  const first = users[dm.recipientIds[0] ?? ''];
  const group = dm.recipientIds.length > 1;
  const title = dmTitle(dm, (id) => users[id]?.displayName ?? 'Unknown');
  const p = first ? presence[first.id] : undefined;
  const sub = group ? `${dm.recipientIds.length + 1} Members` : p?.activity?.text;
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
      label: 'Profile',
      disabled: group,
      onSelect: () =>
        void navigate({
          to: '/channels/$guildId/$channelId',
          params: { guildId: HOME, channelId: dm.id },
        }),
    },
    { label: 'Call', disabled: true },
    ...(group || !userEntries.note
      ? [{ label: 'Add Note', disabled: true } as MenuEntry]
      : [userEntries.note]),
    { type: 'separator' },
    { label: group ? 'Leave Group' : 'Close DM', onSelect: close },
    { label: 'Mark as Read', disabled: !dm.unread, onSelect: () => actions.markRead(dm.id) },
    muted
      ? {
          label: 'Unmute Conversation',
          onSelect: () =>
            void patchChannel(HOME, dm.id, { mute: { muted: false } }).catch(() => undefined),
        }
      : {
          type: 'submenu',
          label: 'Mute Conversation',
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
          {
            label: 'Copy User ID',
            onSelect: () => void navigator.clipboard?.writeText(first.id),
          },
        ] as MenuEntry[])
      : []),
  ];

  return (
    <ContextMenu items={menu} label={`${title} conversation menu`}>
      <div className="lng-dm-wrap" data-muted={muted || undefined}>
        <Link
          to="/channels/$guildId/$channelId"
          params={{ guildId: HOME, channelId: dm.id }}
          className="lng-dm"
          data-unread={dm.unread || undefined}
          data-selected={selected || undefined}
          aria-current={selected ? 'page' : undefined}
          aria-label={`${title}${dm.unread ? ', unread' : ''}${muted ? ', muted' : ''}`}
        >
          <Avatar name={title} size={32} {...(!group && p ? { status: p.status } : {})} />
          <span className="lng-dm-text">
            <span className="lng-dm-name">
              {title}
              {!group && first?.tag && <span className="lng-tag">{first.tag}</span>}
            </span>
            {sub && <span className="lng-dm-sub">{sub}</span>}
          </span>
          <CounterBadge
            count={muted ? 0 : dm.mentionCount}
            noun="mentions"
            className="lng-mention-badge lng-mention-inline"
          />
        </Link>
        <button
          type="button"
          className="lng-icon-btn lng-dm-close"
          aria-label={group ? `Leave group ${title}` : `Close DM with ${title}`}
          onClick={close}
        >
          <Icon name="close" size={16} />
        </button>
      </div>
    </ContextMenu>
  );
}

export function HomeSidebar({ onOpenSwitcher }: { onOpenSwitcher: () => void }) {
  const dms = useDms();
  const counts = useRequestCounts();
  const { channelId, isHome } = useLoungeRoute();
  const navigate = useNavigate();
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
    <div className="lng-sidebar" data-testid="home-sidebar">
      <div className="lng-sidebar-top">
        <button type="button" className="lng-find" onClick={onOpenSwitcher}>
          Find or start a conversation
        </button>
      </div>
      <nav className="lng-home-nav" aria-label="Home">
        <Link
          to="/channels/$guildId"
          params={{ guildId: HOME }}
          className="lng-nav-item"
          data-selected={onFriends || undefined}
          aria-current={onFriends ? 'page' : undefined}
          onClick={() => setHomeView('friends')}
        >
          <Icon name="users" size={20} />
          <span>Friends</span>
          <CounterBadge
            count={counts.friends}
            noun="pending friend requests"
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
          <span>Message Requests</span>
          <CounterBadge
            count={counts.messages}
            noun="message requests"
            className="lng-mention-badge lng-mention-inline"
          />
        </button>
      </nav>
      <div className="lng-dm-head">
        <h2>Direct Messages</h2>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label="Create DM"
          onClick={() => openDialog({ kind: 'createDm' })}
        >
          <Icon name="plus" size={16} />
        </button>
      </div>
      <VirtualList
        className="lng-dm-list"
        aria-label="Direct messages"
        count={sorted.length}
        estimateSize={44}
        getItemKey={(i) => sorted[i]?.id ?? i}
        renderItem={(i) => {
          const dm = sorted[i];
          return dm ? <DmRow dm={dm} selected={dm.id === channelId} /> : null;
        }}
        empty={<p className="lng-muted lng-pad">No conversations yet.</p>}
      />
    </div>
  );
}
