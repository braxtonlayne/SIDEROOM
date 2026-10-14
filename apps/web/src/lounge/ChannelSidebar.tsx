import { Link } from '@tanstack/react-router';
import {
  Avatar,
  ComingSoon,
  ContextMenu,
  CounterBadge,
  DropdownMenu,
  Icon,
  Notice,
  RailPill,
  type IconName,
  type MenuEntry,
} from '@sideroom/ui';
import { useId, useState } from 'react';
import { useVoiceSession } from '../voice/session';
import { useJoinVoice } from './voice/actions';
import { useVoiceMenuEntries } from './voice/useVoiceMenu';
import { groupChannels, isVoiceLike } from './channels';
import {
  useChannels,
  useGuild,
  useUsers,
  useVoiceMembers,
  type Channel,
  type Guild,
  type VoiceMember,
} from './data';
import { useLoungeRoute } from './nav';
import { useGuildMenuCore } from './shell/useGuildMenu';
import { useChannelDnd, type ChannelDropMark } from './shell/useChannelDnd';
import { useChannelMenu } from './shell/useChannelMenu';
import { useLoungeUi } from './store';

export const CHANNEL_ICON: Record<Channel['type'], IconName> = {
  text: 'hash',
  announcement: 'bell',
  voice: 'volume',
  stage: 'mic',
  forum: 'list',
  media: 'image',
  category: 'chevron-down',
  rules: 'book',
};

function VoiceMemberRow({
  channel,
  member: m,
  onError,
}: {
  channel: Channel;
  member: VoiceMember;
  onError: (message: string) => void;
}) {
  const users = useUsers();
  const u = users[m.userId];
  const menu = useVoiceMenuEntries(
    channel.guildId,
    { id: m.userId, name: u?.displayName ?? 'Unknown' },
    onError,
    channel.id,
  );
  if (!u) return null;
  return (
    <li>
      <ContextMenu items={menu} label={`${u.displayName} voice actions`}>
        <div className="lng-voice-member" data-speaking={m.speaking || undefined}>
          <Avatar name={u.displayName} src={u.avatarUrl ?? null} size={24} speaking={m.speaking} />
          <span className="lng-voice-name">{u.displayName}</span>
          {m.streaming && <span className="lng-live">LIVE</span>}
          {m.video && <Icon name="video" size={16} label="Camera on" />}
          {m.muted && <Icon name="mic-off" size={16} label="Muted" />}
          {m.deafened && <Icon name="volume-off" size={16} label="Deafened" />}
        </div>
      </ContextMenu>
    </li>
  );
}

function VoiceMembers({ channel, onError }: { channel: Channel; onError: (m: string) => void }) {
  const members = useVoiceMembers(channel.id);
  if (!members.length) return null;
  return (
    <ul className="lng-voice-members" aria-label={`Connected to ${channel.name}`}>
      {members.map((m) => (
        <VoiceMemberRow key={m.userId} channel={channel} member={m} onError={onError} />
      ))}
    </ul>
  );
}

interface Dnd {
  props: (id: string) => Record<string, unknown>;
  mark: ChannelDropMark;
  dragging: string | null;
}

function ChannelRow({
  channel,
  guild,
  channels,
  selected,
  dnd,
  onError,
}: {
  channel: Channel;
  guild: Guild;
  channels: Channel[];
  selected: boolean;
  dnd: Dnd;
  onError: (m: string) => void;
}) {
  const joinVoice = useJoinVoice();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const connected = useVoiceSession((s) => s.channelId === channel.id && s.status !== 'idle');
  const members = useVoiceMembers(channel.id);
  const menu = useChannelMenu(channel, guild, channels, onError);
  const nameId = useId();
  const icon = channel.locked ? 'lock' : CHANNEL_ICON[channel.type];
  const state = selected ? 'selected' : channel.unread && !channel.muted ? 'unread' : 'none';
  const cls = 'lng-channel';
  const attrs = {
    'data-unread': (channel.unread && !channel.muted) || undefined,
    'data-muted': channel.muted || undefined,
    'data-selected': selected || undefined,
  };
  const label = `${channel.name}${channel.unread ? ', unread' : ''}${channel.mentionCount ? `, ${channel.mentionCount} mentions` : ''}${channel.muted ? ', muted' : ''}`;
  return (
    <li
      className="lng-channel-item"
      data-dragging={dnd.dragging === channel.id || undefined}
      data-drop={dnd.mark?.id === channel.id ? dnd.mark.zone : undefined}
      {...dnd.props(channel.id)}
    >
      <ContextMenu items={menu} label={`${channel.name} menu`}>
        {isVoiceLike(channel) ? (
          <>
            <button
              type="button"
              className={cls}
              {...attrs}
              data-connected={connected || undefined}
              aria-label={`${channel.name}, ${channel.type} channel${connected ? ', connected' : ''}`}
              onClick={() => joinVoice(channel)}
            >
              <Icon name={icon} size={20} />
              <span className="lng-channel-name" id={nameId}>
                {channel.name}
              </span>
              {channel.userLimit ? (
                <span className="lng-voice-limit">
                  {members.length}/{channel.userLimit}
                </span>
              ) : null}
            </button>
            <VoiceMembers channel={channel} onError={onError} />
          </>
        ) : (
          <>
            <RailPill state={state} />
            <Link
              to="/channels/$guildId/$channelId"
              params={{ guildId: guild.id, channelId: channel.id }}
              className={cls}
              aria-label={label}
              aria-current={selected ? 'page' : undefined}
              draggable={false}
              {...attrs}
            >
              <Icon name={icon} size={20} />
              <span className="lng-channel-name" id={nameId}>
                {channel.name}
              </span>
              <CounterBadge
                count={channel.muted ? 0 : channel.mentionCount}
                noun="mentions"
                className="lng-mention-badge lng-mention-inline"
              />
            </Link>
          </>
        )}
        <span className="lng-channel-actions">
          {(channel.permissions?.createInvite ?? guild.permissions.createInvite) && (
            <button
              type="button"
              className="lng-icon-btn"
              aria-label="Create Invite"
              aria-describedby={nameId}
              onClick={() =>
                openDialog({ kind: 'invite', guildId: guild.id, channelId: channel.id })
              }
            >
              <Icon name="users" size={16} />
            </button>
          )}
          {(channel.permissions?.manageChannels ?? guild.permissions.manageChannels) && (
            <button
              type="button"
              className="lng-icon-btn"
              aria-label="Edit Channel"
              aria-describedby={nameId}
              onClick={() =>
                openDialog({ kind: 'editChannel', guildId: guild.id, channelId: channel.id })
              }
            >
              <Icon name="settings" size={16} />
            </button>
          )}
        </span>
      </ContextMenu>
    </li>
  );
}

function CategoryHead({
  category,
  guild,
  channels,
  collapsed,
  dnd,
  onError,
}: {
  category: Channel;
  guild: Guild;
  channels: Channel[];
  collapsed: boolean;
  dnd: Dnd;
  onError: (m: string) => void;
}) {
  const toggle = useLoungeUi((s) => s.toggleCategory);
  const openDialog = useLoungeUi((s) => s.openDialog);
  const menu = useChannelMenu(category, guild, channels, onError);
  return (
    <ContextMenu items={menu} label={`${category.name} category menu`}>
      <div
        className="lng-category-head"
        data-muted={category.muted || undefined}
        data-dragging={dnd.dragging === category.id || undefined}
        data-drop={dnd.mark?.id === category.id ? dnd.mark.zone : undefined}
        {...dnd.props(category.id)}
      >
        <button
          type="button"
          className="lng-category-btn"
          aria-expanded={!collapsed}
          onClick={() => toggle(category.id)}
        >
          <Icon name={collapsed ? 'chevron-right' : 'chevron-down'} size={16} />
          <span>{category.name}</span>
        </button>
        {guild.permissions.manageChannels && (
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={`Create Channel in ${category.name}`}
            onClick={() =>
              openDialog({ kind: 'createChannel', guildId: guild.id, parentId: category.id })
            }
          >
            <Icon name="plus" size={16} />
          </button>
        )}
      </div>
    </ContextMenu>
  );
}

function GuildHeader({ guild }: { guild: Guild }) {
  const core = useGuildMenuCore(guild);
  const [coming, setComing] = useState<string | null>(null);
  const soon = (title: string) => () => setComing(title);
  const p = guild.permissions;
  const items: MenuEntry[] = [
    { label: 'Server Support', onSelect: soon('Server Support') },
    ...(p.createInvite
      ? [{ ...(core.invite as object), label: 'Invite to Server' } as MenuEntry]
      : []),
    ...((p.serverSettings ?? p.manageGuild) ? [core.settings] : []),
    ...(p.manageChannels
      ? [
          core.createChannel,
          core.createCategory,
          { label: 'Create Event', onSelect: soon('Create Event') },
        ]
      : []),
    { label: 'App Directory', onSelect: soon('App Directory') },
    core.notifications,
    { label: 'Privacy Settings', onSelect: soon('Privacy Settings') },
    { label: 'Edit Per-server Profile', onSelect: soon('Edit Per-server Profile') },
    core.mute,
    core.hideMuted,
    {
      type: 'submenu',
      label: 'Copy Server Info',
      items: [
        core.copyId,
        {
          label: 'Copy Server Link',
          onSelect: () =>
            void navigator.clipboard?.writeText(`${location.origin}/channels/${guild.id}`),
        },
      ],
    },
    ...(p.moderate
      ? [{ label: 'Security Actions', onSelect: soon('Security Actions') } as MenuEntry]
      : []),
    ...(core.isOwner ? [] : [{ type: 'separator' } as MenuEntry, core.leave]),
  ];
  return (
    <>
      <DropdownMenu
        label={`${guild.name} server menu`}
        items={items}
        trigger={
          <button type="button" className="lng-guild-header">
            <span className="lng-guild-title">{guild.name}</span>
            <Icon name="chevron-down" size={16} />
          </button>
        }
      />
      <ComingSoon open={coming !== null} onClose={() => setComing(null)} title={coming ?? ''}>
        <p>This server action ships with its own inventory item (LNG-GLD-019..034).</p>
      </ComingSoon>
    </>
  );
}

export function ChannelSidebar({ guildId }: { guildId: string }) {
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const { channelId } = useLoungeRoute();
  const collapsed = useLoungeUi((s) => s.collapsedCategories);
  const hideMuted = useLoungeUi((s) => !!s.hideMutedChannels[guildId]);
  const [error, setError] = useState<string | null>(null);
  const dnd = useChannelDnd(guildId, channels, !!guild?.permissions.manageChannels, setError);
  if (!guild) return <div className="lng-sidebar" />;
  const groups = groupChannels(channels, { hideMuted, activeId: channelId });
  return (
    <div className="lng-sidebar" data-testid="channel-sidebar">
      <GuildHeader guild={guild} />
      {error && (
        <Notice variant="danger" onDismiss={() => setError(null)}>
          {error}
        </Notice>
      )}
      <nav className="lng-channels" aria-label={`${guild.name} channels`}>
        {groups.length === 0 && <p className="lng-muted lng-pad">No channels yet.</p>}
        {groups.map((g) => {
          const isCollapsed = !!g.category && !!collapsed[g.category.id];
          const shown = isCollapsed
            ? g.channels.filter((c) => c.id === channelId || (c.unread && !c.muted))
            : g.channels;
          return (
            <section
              key={g.category?.id ?? 'loose'}
              className="lng-category"
              data-collapsed={isCollapsed || undefined}
            >
              {g.category && (
                <CategoryHead
                  category={g.category}
                  guild={guild}
                  channels={channels}
                  collapsed={isCollapsed}
                  dnd={dnd}
                  onError={setError}
                />
              )}
              <ul className="lng-channel-list">
                {shown.map((c) => (
                  <ChannelRow
                    key={c.id}
                    channel={c}
                    guild={guild}
                    channels={channels}
                    selected={c.id === channelId}
                    dnd={dnd}
                    onError={setError}
                  />
                ))}
              </ul>
            </section>
          );
        })}
      </nav>
    </div>
  );
}
