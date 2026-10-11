import { Link } from '@tanstack/react-router';
import {
  Avatar,
  CounterBadge,
  DropdownMenu,
  Icon,
  type IconName,
  type MenuEntry,
} from '@sideroom/ui';
import { useState } from 'react';
import { ComingSoon } from '@sideroom/ui';
import { RailPill } from '@sideroom/ui';
import { groupChannels, isVoiceLike } from './channels';
import {
  useChannels,
  useCurrentUser,
  useGuild,
  useUsers,
  useVoiceMembers,
  type Channel,
  type Guild,
} from './data';
import { useLoungeRoute } from './nav';
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

function VoiceMembers({ channel }: { channel: Channel }) {
  const members = useVoiceMembers(channel.id);
  const users = useUsers();
  if (!members.length) return null;
  return (
    <ul className="lng-voice-members" aria-label={`Connected to ${channel.name}`}>
      {members.map((m) => {
        const u = users[m.userId];
        if (!u) return null;
        return (
          <li key={m.userId} className="lng-voice-member">
            <Avatar name={u.displayName} size={24} speaking={m.speaking} />
            <span className="lng-voice-name">{u.displayName}</span>
            {m.streaming && <span className="lng-live">LIVE</span>}
            {m.video && <Icon name="video" size={16} label="Camera on" />}
            {m.muted && <Icon name="mic-off" size={16} label="Muted" />}
            {m.deafened && <Icon name="volume-off" size={16} label="Deafened" />}
          </li>
        );
      })}
    </ul>
  );
}

function ChannelRow({
  channel,
  guild,
  selected,
}: {
  channel: Channel;
  guild: Guild;
  selected: boolean;
}) {
  const joinVoice = useLoungeUi((s) => s.joinVoice);
  const connected = useLoungeUi((s) => s.voice?.channelId === channel.id);
  const members = useVoiceMembers(channel.id);
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
    <li className="lng-channel-item">
      {isVoiceLike(channel) ? (
        <>
          <button
            type="button"
            className={cls}
            {...attrs}
            data-connected={connected || undefined}
            aria-label={`${channel.name}, ${channel.type} channel${connected ? ', connected' : ''}`}
            onClick={() =>
              joinVoice({ guildId: guild.id, channelId: channel.id, name: channel.name })
            }
          >
            <Icon name={icon} size={20} />
            <span className="lng-channel-name">{channel.name}</span>
            {channel.userLimit ? (
              <span className="lng-voice-limit">
                {members.length}/{channel.userLimit}
              </span>
            ) : null}
          </button>
          <VoiceMembers channel={channel} />
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
            {...attrs}
          >
            <Icon name={icon} size={20} />
            <span className="lng-channel-name">{channel.name}</span>
            <CounterBadge
              count={channel.muted ? 0 : channel.mentionCount}
              noun="mentions"
              className="lng-mention-badge lng-mention-inline"
            />
          </Link>
        </>
      )}
    </li>
  );
}

function GuildHeader({ guild }: { guild: Guild }) {
  const hideMuted = useLoungeUi((s) => !!s.hideMutedChannels[guild.id]);
  const toggleHideMuted = useLoungeUi((s) => s.toggleHideMuted);
  const p = guild.permissions;
  const me = useCurrentUser();
  const [coming, setComing] = useState<string | null>(null);
  const soon = (title: string) => () => setComing(title);
  const items: MenuEntry[] = [
    { label: 'Server Support', onSelect: soon('Server Support') },
    ...(p.createInvite
      ? [{ label: 'Invite to Server', onSelect: soon('Invite to Server') } as MenuEntry]
      : []),
    ...(p.manageGuild
      ? [{ label: 'Server Settings', onSelect: soon('Server Settings') } as MenuEntry]
      : []),
    ...(p.manageChannels
      ? ([
          { label: 'Create Channel', onSelect: soon('Create Channel') },
          { label: 'Create Category', onSelect: soon('Create Category') },
          { label: 'Create Event', onSelect: soon('Create Event') },
        ] as MenuEntry[])
      : []),
    { label: 'App Directory', onSelect: soon('App Directory') },
    { label: 'Notification Settings', onSelect: soon('Notification Settings') },
    { label: 'Privacy Settings', onSelect: soon('Privacy Settings') },
    { label: 'Edit Per-server Profile', onSelect: soon('Edit Per-server Profile') },
    {
      type: 'checkbox',
      label: 'Hide Muted Channels',
      checked: hideMuted,
      onCheckedChange: () => toggleHideMuted(guild.id),
    },
    {
      type: 'submenu',
      label: 'Copy Server Info',
      items: [
        { label: 'Copy Server ID', onSelect: () => void navigator.clipboard?.writeText(guild.id) },
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
    ...(guild.ownerId === me.id
      ? []
      : [
          { type: 'separator' } as MenuEntry,
          { label: 'Leave Server', danger: true, onSelect: soon('Leave Server') } as MenuEntry,
        ]),
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
  const toggle = useLoungeUi((s) => s.toggleCategory);
  const hideMuted = useLoungeUi((s) => !!s.hideMutedChannels[guildId]);
  const [creating, setCreating] = useState(false);
  if (!guild) return <div className="lng-sidebar" />;
  const groups = groupChannels(channels, { hideMuted, activeId: channelId });
  return (
    <div className="lng-sidebar" data-testid="channel-sidebar">
      <GuildHeader guild={guild} />
      <nav className="lng-channels" aria-label={`${guild.name} channels`}>
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
                <div className="lng-category-head">
                  <button
                    type="button"
                    className="lng-category-btn"
                    aria-expanded={!isCollapsed}
                    onClick={() => toggle(g.category!.id)}
                  >
                    <Icon name={isCollapsed ? 'chevron-right' : 'chevron-down'} size={16} />
                    <span>{g.category.name}</span>
                  </button>
                  {guild.permissions.manageChannels && (
                    <button
                      type="button"
                      className="lng-icon-btn"
                      aria-label={`Create Channel in ${g.category.name}`}
                      onClick={() => setCreating(true)}
                    >
                      <Icon name="plus" size={16} />
                    </button>
                  )}
                </div>
              )}
              <ul className="lng-channel-list">
                {shown.map((c) => (
                  <ChannelRow key={c.id} channel={c} guild={guild} selected={c.id === channelId} />
                ))}
              </ul>
            </section>
          );
        })}
      </nav>
      <ComingSoon open={creating} onClose={() => setCreating(false)} title="Create Channel">
        <p>The Create Channel modal ships with LNG-CHN-015.</p>
      </ComingSoon>
    </div>
  );
}
