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
import { RovingNav } from '../a11y/RovingNav';
import { useT } from '../i18n';
import { useVoiceSession } from '../voice/session';
import { useJoinVoice } from './voice/actions';
import './voice/voice-p10.css';
import { useVoicePerms } from './voice/perms';
import { useVoiceUserMenu } from './voice/useVoiceUserMenu';
import { dropVoiceUser, isVoiceDrag, readVoiceDrag, startVoiceDrag } from './voice/voiceDrag';
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
import { NestedThreads } from './threads/NestedThreads';
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
  const t = useT();
  const users = useUsers();
  const u = users[m.userId];
  const menu = useVoiceUserMenu(
    channel.guildId,
    { id: m.userId, name: u?.displayName ?? t('lounge.channelSidebar.unknown') },
    onError,
    channel.id,
  );
  const canMove = useVoicePerms(channel.guildId, channel.id).moveMembers;
  if (!u) return null;
  return (
    <li
      draggable={canMove || undefined}
      data-draggable={canMove || undefined}
      onDragStart={
        canMove
          ? (e) => {
              e.stopPropagation();
              startVoiceDrag(e, {
                userId: m.userId,
                guildId: channel.guildId,
                fromChannelId: channel.id,
              });
            }
          : undefined
      }
    >
      <ContextMenu
        items={menu}
        label={t('lounge.channelSidebar.voiceActions', { name: u.displayName })}
      >
        <div className="lng-voice-member" data-speaking={m.speaking || undefined}>
          <Avatar name={u.displayName} src={u.avatarUrl ?? null} size={24} speaking={m.speaking} />
          <span className="lng-voice-name">{u.displayName}</span>
          {m.streaming && <span className="lng-live">{t('lounge.channelSidebar.live')}</span>}
          {m.video && <Icon name="video" size={16} label={t('lounge.channelSidebar.cameraOn')} />}
          {m.serverMuted ? (
            <Icon name="mic-off" size={16} label={t('lounge.channelSidebar.serverMuted')} />
          ) : (
            m.muted && <Icon name="mic-off" size={16} label={t('lounge.channelSidebar.muted')} />
          )}
          {m.serverDeafened ? (
            <Icon name="volume-off" size={16} label={t('lounge.channelSidebar.serverDeafened')} />
          ) : (
            m.deafened && (
              <Icon name="volume-off" size={16} label={t('lounge.channelSidebar.deafened')} />
            )
          )}
        </div>
      </ContextMenu>
    </li>
  );
}

function VoiceMembers({ channel, onError }: { channel: Channel; onError: (m: string) => void }) {
  const t = useT();
  const members = useVoiceMembers(channel.id);
  if (!members.length) return null;
  return (
    <ul
      className="lng-voice-members"
      aria-label={t('lounge.channelSidebar.connectedTo', { name: channel.name })}
    >
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
  const t = useT();
  const joinVoice = useJoinVoice();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const connected = useVoiceSession((s) => s.channelId === channel.id && s.status !== 'idle');
  const members = useVoiceMembers(channel.id);
  const menu = useChannelMenu(channel, guild, channels, onError);
  const [voiceDropOver, setVoiceDropOver] = useState(false);
  const nameId = useId();
  const icon = channel.locked ? 'lock' : CHANNEL_ICON[channel.type];
  const state = selected ? 'selected' : channel.unread && !channel.muted ? 'unread' : 'none';
  const cls = 'lng-channel';
  const attrs = {
    'data-unread': (channel.unread && !channel.muted) || undefined,
    'data-muted': channel.muted || undefined,
    'data-selected': selected || undefined,
  };
  const label = `${channel.name}${channel.unread ? t('lounge.channelSidebar.unreadSuffix') : ''}${channel.mentionCount ? t('lounge.channelSidebar.mentionsSuffix', { count: channel.mentionCount }) : ''}${channel.muted ? t('lounge.channelSidebar.mutedSuffix') : ''}`;
  return (
    <li
      className="lng-channel-item"
      data-dragging={dnd.dragging === channel.id || undefined}
      data-drop={dnd.mark?.id === channel.id ? dnd.mark.zone : undefined}
      {...dnd.props(channel.id)}
    >
      <ContextMenu
        items={menu}
        label={t('lounge.channelSidebar.channelMenu', { name: channel.name })}
      >
        {isVoiceLike(channel) ? (
          <>
            <button
              type="button"
              className={cls}
              {...attrs}
              data-connected={connected || undefined}
              aria-label={
                connected
                  ? t('lounge.channelSidebar.voiceChannelConnected', {
                      name: channel.name,
                      type: channel.type,
                    })
                  : t('lounge.channelSidebar.voiceChannel', {
                      name: channel.name,
                      type: channel.type,
                    })
              }
              onClick={() => joinVoice(channel)}
              data-voice-drop={voiceDropOver || undefined}
              onDragOver={(e) => {
                if (!isVoiceDrag(e)) return;
                e.preventDefault();
                e.stopPropagation();
                e.dataTransfer.dropEffect = 'move';
                setVoiceDropOver(true);
              }}
              onDragLeave={() => setVoiceDropOver(false)}
              onDrop={(e) => {
                const data = readVoiceDrag(e);
                if (!data) return;
                e.preventDefault();
                e.stopPropagation();
                setVoiceDropOver(false);
                void dropVoiceUser(data, channel.guildId, channel.id).catch((err: unknown) =>
                  onError(
                    err instanceof Error ? err.message : t('lounge.channelSidebar.moveFailed'),
                  ),
                );
              }}
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
                noun={t('lounge.channelSidebar.mentionsNoun')}
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
              aria-label={t('lounge.channelSidebar.createInvite')}
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
              aria-label={t('lounge.channelSidebar.editChannel')}
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
      {!isVoiceLike(channel) && <NestedThreads guildId={guild.id} channelId={channel.id} />}
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
  const t = useT();
  const toggle = useLoungeUi((s) => s.toggleCategory);
  const openDialog = useLoungeUi((s) => s.openDialog);
  const menu = useChannelMenu(category, guild, channels, onError);
  return (
    <ContextMenu
      items={menu}
      label={t('lounge.channelSidebar.categoryMenu', { name: category.name })}
    >
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
            aria-label={t('lounge.channelSidebar.createChannelIn', { name: category.name })}
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
  const t = useT();
  const core = useGuildMenuCore(guild);
  const [coming, setComing] = useState<string | null>(null);
  const soon = (title: string) => () => setComing(title);
  const p = guild.permissions;
  const items: MenuEntry[] = [
    {
      label: t('lounge.channelSidebar.serverSupport'),
      onSelect: soon(t('lounge.channelSidebar.serverSupport')),
    },
    ...(p.createInvite
      ? [
          {
            ...(core.invite as object),
            label: t('lounge.channelSidebar.inviteToServer'),
          } as MenuEntry,
        ]
      : []),
    ...(guild.isStaff ? [core.platformAdmin] : []),
    ...((p.serverSettings ?? p.manageGuild) ? [core.settings] : []),
    ...(p.manageChannels
      ? [
          core.createChannel,
          core.createCategory,
          {
            label: t('lounge.channelSidebar.createEvent'),
            onSelect: soon(t('lounge.channelSidebar.createEvent')),
          },
        ]
      : []),
    {
      label: t('lounge.channelSidebar.appDirectory'),
      onSelect: soon(t('lounge.channelSidebar.appDirectory')),
    },
    core.notifications,
    {
      label: t('lounge.channelSidebar.privacySettings'),
      onSelect: soon(t('lounge.channelSidebar.privacySettings')),
    },
    {
      label: t('lounge.channelSidebar.editPerServerProfile'),
      onSelect: soon(t('lounge.channelSidebar.editPerServerProfile')),
    },
    core.mute,
    core.hideMuted,
    {
      type: 'submenu',
      label: t('lounge.channelSidebar.copyServerInfo'),
      items: [
        core.copyId,
        {
          label: t('lounge.channelSidebar.copyServerLink'),
          onSelect: () =>
            void navigator.clipboard?.writeText(`${location.origin}/channels/${guild.id}`),
        },
      ],
    },
    ...(p.moderate
      ? [
          {
            label: t('lounge.channelSidebar.securityActions'),
            onSelect: soon(t('lounge.channelSidebar.securityActions')),
          } as MenuEntry,
        ]
      : []),
    ...(core.isOwner ? [] : [{ type: 'separator' } as MenuEntry, core.leave]),
  ];
  return (
    <>
      <DropdownMenu
        label={t('lounge.channelSidebar.serverMenu', { name: guild.name })}
        items={items}
        trigger={
          <button type="button" className="lng-guild-header">
            <span className="lng-guild-title">{guild.name}</span>
            <Icon name="chevron-down" size={16} />
          </button>
        }
      />
      <ComingSoon open={coming !== null} onClose={() => setComing(null)} title={coming ?? ''}>
        <p>{t('lounge.channelSidebar.serverActionSoon')}</p>
      </ComingSoon>
    </>
  );
}

export function ChannelSidebar({ guildId }: { guildId: string }) {
  const t = useT();
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
      <RovingNav
        className="lng-channels"
        aria-label={t('lounge.channelSidebar.channels', { name: guild.name })}
        selector=":is(a, button).lng-channel, .lng-category-btn"
      >
        {groups.length === 0 && (
          <p className="lng-muted lng-pad">{t('lounge.channelSidebar.noChannels')}</p>
        )}
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
      </RovingNav>
    </div>
  );
}
