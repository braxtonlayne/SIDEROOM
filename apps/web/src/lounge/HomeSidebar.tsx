import { Link } from '@tanstack/react-router';
import { Avatar, CounterBadge, Icon, VirtualList } from '@sideroom/ui';
import { useState } from 'react';
import { ComingSoon } from '@sideroom/ui';
import { dmTitle, HOME } from './channels';
import { usePresenceMap, useDms, useRequestCounts, useUsers, type DmChannel } from './data';
import { useLoungeRoute } from './nav';

function DmRow({ dm, selected }: { dm: DmChannel; selected: boolean }) {
  const users = useUsers();
  const presence = usePresenceMap();
  const first = users[dm.recipientIds[0] ?? ''];
  const group = dm.recipientIds.length > 1;
  const title = dmTitle(dm, (id) => users[id]?.displayName ?? 'Unknown');
  const p = first ? presence[first.id] : undefined;
  const sub = group ? `${dm.recipientIds.length + 1} Members` : p?.activity?.text;
  return (
    <Link
      to="/channels/$guildId/$channelId"
      params={{ guildId: HOME, channelId: dm.id }}
      className="lng-dm"
      data-unread={dm.unread || undefined}
      data-selected={selected || undefined}
      aria-current={selected ? 'page' : undefined}
      aria-label={`${title}${dm.unread ? ', unread' : ''}`}
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
        count={dm.mentionCount}
        noun="mentions"
        className="lng-mention-badge lng-mention-inline"
      />
    </Link>
  );
}

export function HomeSidebar({ onOpenSwitcher }: { onOpenSwitcher: () => void }) {
  const dms = useDms();
  const counts = useRequestCounts();
  const { channelId, isHome } = useLoungeRoute();
  const [coming, setComing] = useState<string | null>(null);
  const sorted = [...dms].sort((a, b) => b.lastMessageAt.localeCompare(a.lastMessageAt));
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
          data-selected={(isHome && !channelId) || undefined}
          aria-current={isHome && !channelId ? 'page' : undefined}
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
          onClick={() => setComing('Message Requests')}
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
          onClick={() => setComing('Create DM')}
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
      <ComingSoon open={coming !== null} onClose={() => setComing(null)} title={coming ?? ''}>
        <p>This arrives with LNG-HOME / LNG-DM items.</p>
      </ComingSoon>
    </div>
  );
}
