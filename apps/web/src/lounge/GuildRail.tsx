import { Link, useNavigate } from '@tanstack/react-router';
import { Avatar, ContextMenu, CounterBadge, Icon, Tooltip, type MenuEntry } from '@sideroom/ui';
import { useState, type ReactNode } from 'react';
import { RailPill, type RailPillState } from '@sideroom/ui';
import { ComingSoon } from '@sideroom/ui';
import { firstTextChannel, HOME } from './channels';
import {
  useAllChannels,
  useCurrentUser,
  useDms,
  useGuilds,
  useLoungeActions,
  useRail,
  useUsers,
  type Guild,
  type RailEntry,
} from './data';
import { useLoungeRoute } from './nav';
import { useLoungeUi } from './store';

export function acronym(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .map((w) => w[0])
    .join('')
    .slice(0, 3)
    .toUpperCase();
}

function GuildLink({
  guild,
  children,
  className,
}: {
  guild: Guild;
  children: ReactNode;
  className: string;
}) {
  const channels = useAllChannels()[guild.id] ?? [];
  const remembered = useLoungeUi((s) => s.lastChannelByGuild[guild.id]);
  const target =
    (remembered && channels.some((c) => c.id === remembered)
      ? remembered
      : firstTextChannel(channels)?.id) ?? null;
  const label =
    guild.name +
    (guild.mentionCount ? `, ${guild.mentionCount} mentions` : guild.unread ? ', unread' : '');
  return target ? (
    <Link
      to="/channels/$guildId/$channelId"
      params={{ guildId: guild.id, channelId: target }}
      className={className}
      aria-label={label}
    >
      {children}
    </Link>
  ) : (
    <Link
      to="/channels/$guildId"
      params={{ guildId: guild.id }}
      className={className}
      aria-label={label}
    >
      {children}
    </Link>
  );
}

function GuildIcon({ guild, selected }: { guild: Guild; selected: boolean }) {
  const inVoice = useLoungeUi((s) => s.voice?.guildId === guild.id);
  const me = useCurrentUser();
  const voice = inVoice || guild.voiceConnected;
  const pill: RailPillState = selected ? 'selected' : guild.unread ? 'unread' : 'none';
  const actions = useLoungeActions();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const navigate = useNavigate();
  const menu: MenuEntry[] = [
    {
      label: 'Mark As Read',
      disabled: !guild.unread,
      onSelect: () => actions.markGuildRead(guild.id),
    },
    {
      label: 'Invite People',
      disabled: !guild.permissions.createInvite,
      onSelect: () => openDialog({ kind: 'invite', guildId: guild.id }),
    },
    { label: 'Mute Server', disabled: true },
    { label: 'Notification Settings', disabled: true },
    { type: 'separator' },
    {
      label: 'Server Settings',
      disabled: !(guild.permissions.serverSettings ?? guild.permissions.manageGuild),
      onSelect: () =>
        void navigate({ to: '/settings/server/$guildId', params: { guildId: guild.id } }),
    },
    {
      label: 'Create Channel',
      disabled: !guild.permissions.manageChannels,
      onSelect: () => openDialog({ kind: 'createChannel', guildId: guild.id }),
    },
    { type: 'separator' },
    { label: 'Leave Server', danger: true, disabled: guild.ownerId === me.id },
    { label: 'Copy Server ID', onSelect: () => void navigator.clipboard?.writeText(guild.id) },
  ];
  return (
    <ContextMenu items={menu} label={`${guild.name} menu`}>
      <div className="lng-rail-item" data-selected={selected || undefined}>
        <RailPill state={pill} />
        <Tooltip content={guild.name + (voice ? ' (in voice)' : '')} placement="right" delay={150}>
          <GuildLink
            guild={guild}
            className={`lng-guild${guild.isStaff ? ' lng-guild-staff' : ''}`}
          >
            {guild.iconUrl ? (
              <img src={guild.iconUrl} alt="" />
            ) : (
              <span aria-hidden="true">{acronym(guild.name)}</span>
            )}
            {voice && (
              <span className="lng-guild-voice" title="Connected to voice">
                <Icon name="volume" size={16} />
              </span>
            )}
            {guild.isNew && <span className="lng-new-badge">NEW</span>}
          </GuildLink>
        </Tooltip>
        <CounterBadge count={guild.mentionCount} noun="mentions" className="lng-mention-badge" />
      </div>
    </ContextMenu>
  );
}

function Folder({
  entry,
  guilds,
  activeGuildId,
}: {
  entry: Extract<RailEntry, { kind: 'folder' }>;
  guilds: Guild[];
  activeGuildId?: string;
}) {
  const open = useLoungeUi((s) => !!s.openFolders[entry.id]);
  const toggle = useLoungeUi((s) => s.toggleFolder);
  const members = entry.guildIds
    .map((id) => guilds.find((g) => g.id === id))
    .filter((g): g is Guild => !!g);
  const unread = members.some((g) => g.unread);
  const mentions = members.reduce((n, g) => n + g.mentionCount, 0);
  const name = entry.name ?? members.map((g) => g.name).join(', ');
  return (
    <li
      className="lng-folder"
      data-open={open || undefined}
      style={{ ['--folder-color' as string]: entry.color }}
    >
      <div className="lng-rail-item">
        <RailPill state={unread && !open ? 'unread' : 'none'} />
        <Tooltip content={name} placement="right" delay={150}>
          <button
            type="button"
            className="lng-guild lng-folder-btn"
            aria-expanded={open}
            aria-label={`${name} folder${mentions ? `, ${mentions} mentions` : unread ? ', unread' : ''}`}
            onClick={() => toggle(entry.id)}
          >
            {open ? (
              <Icon name="chevron-down" size={20} />
            ) : (
              <span className="lng-folder-grid" aria-hidden="true">
                {members.slice(0, 4).map((g) => (
                  <span key={g.id}>{acronym(g.name)[0]}</span>
                ))}
              </span>
            )}
          </button>
        </Tooltip>
        {!open && <CounterBadge count={mentions} noun="mentions" className="lng-mention-badge" />}
      </div>
      {open && (
        <ul className="lng-folder-list" aria-label={name}>
          {members.map((g) => (
            <li key={g.id}>
              <GuildIcon guild={g} selected={g.id === activeGuildId} />
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

export function GuildRail() {
  const guilds = useGuilds();
  const rail = useRail();
  const dms = useDms();
  const users = useUsers();
  const { guildId } = useLoungeRoute();
  const [modal, setModal] = useState<null | 'discover'>(null);
  const openDialog = useLoungeUi((s) => s.openDialog);
  const dmMentions = dms.reduce((n, d) => n + d.mentionCount, 0);
  const unreadDms = dms.filter((d) => d.unread);
  const homeSelected = guildId === HOME || guildId === undefined;
  return (
    <nav className="lng-rail" aria-label="Servers">
      <ul className="lng-rail-list">
        <li>
          <div className="lng-rail-item" data-selected={homeSelected || undefined}>
            <RailPill state={homeSelected ? 'selected' : unreadDms.length ? 'unread' : 'none'} />
            <Tooltip content="Direct Messages" placement="right" delay={150}>
              <Link
                to="/channels/$guildId"
                params={{ guildId: HOME }}
                className="lng-guild lng-home"
                aria-label={`Direct Messages${dmMentions ? `, ${dmMentions} mentions` : ''}`}
              >
                <Icon name="door" size={24} />
              </Link>
            </Tooltip>
            <CounterBadge count={dmMentions} noun="mentions" className="lng-mention-badge" />
          </div>
        </li>
        {unreadDms.map((d) => {
          const u = users[d.recipientIds[0] ?? ''];
          const name = d.name ?? u?.displayName ?? 'Direct message';
          return (
            <li key={d.id}>
              <div className="lng-rail-item">
                <Tooltip content={name} placement="right" delay={150}>
                  <Link
                    to="/channels/$guildId/$channelId"
                    params={{ guildId: HOME, channelId: d.id }}
                    className="lng-guild lng-dm-unread"
                    aria-label={`${name}, unread direct message`}
                  >
                    <Avatar name={name} size={40} />
                  </Link>
                </Tooltip>
                <CounterBadge
                  count={d.mentionCount}
                  noun="mentions"
                  className="lng-mention-badge"
                />
              </div>
            </li>
          );
        })}
        <li aria-hidden="true">
          <hr className="lng-rail-sep" />
        </li>
        {rail.map((e) => {
          if (e.kind === 'folder')
            return <Folder key={e.id} entry={e} guilds={guilds} activeGuildId={guildId} />;
          const g = guilds.find((x) => x.id === e.guildId);
          return g ? (
            <li key={g.id}>
              <GuildIcon guild={g} selected={g.id === guildId} />
            </li>
          ) : null;
        })}
        <li>
          <div className="lng-rail-item">
            <Tooltip content="Add a Server" placement="right" delay={150}>
              <button
                type="button"
                className="lng-guild lng-rail-action"
                aria-label="Add a Server"
                onClick={() => openDialog({ kind: 'addServer' })}
              >
                <Icon name="plus" size={24} />
              </button>
            </Tooltip>
          </div>
        </li>
        <li>
          <div className="lng-rail-item">
            <Tooltip content="Discover" placement="right" delay={150}>
              <button
                type="button"
                className="lng-guild lng-rail-action"
                aria-label="Discover"
                onClick={() => setModal('discover')}
              >
                <Icon name="compass" size={24} />
              </button>
            </Tooltip>
          </div>
        </li>
      </ul>
      <ComingSoon
        open={modal === 'discover'}
        onClose={() => setModal(null)}
        title="Server Directory"
      >
        <p>The Server Directory ships with LNG-GLD-028.</p>
      </ComingSoon>
    </nav>
  );
}
