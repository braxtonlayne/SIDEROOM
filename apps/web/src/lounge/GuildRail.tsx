import { Link } from '@tanstack/react-router';
import { Avatar, ContextMenu, CounterBadge, Icon, Tooltip, type MenuEntry } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { RailPill, type RailPillState } from '@sideroom/ui';
import { RovingNav } from '../a11y/RovingNav';
import { useT } from '../i18n';
import { firstTextChannel, HOME } from './channels';
import {
  useAllChannels,
  useDms,
  useGuilds,
  useLoungeActions,
  useRail,
  useUsers,
  type Guild,
  type RailEntry,
} from './data';
import { useLoungeRoute } from './nav';
import { useVoiceSession } from '../voice/session';
import { useLoungeUi } from './store';
import { useGuildMenuCore, useLoadNotifSettings } from './shell/useGuildMenu';
import { useRailDnd, type DropMark, type DropTarget } from './shell/useRailDnd';
import { addToFolder, moveBy, removeFromFolder, deleteFolder } from './shell/railLayout';

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
  const t = useT();
  const channels = useAllChannels()[guild.id] ?? [];
  const remembered = useLoungeUi((s) => s.lastChannelByGuild[guild.id]);
  const target =
    (remembered && channels.some((c) => c.id === remembered)
      ? remembered
      : firstTextChannel(channels)?.id) ?? null;
  const label =
    guild.name +
    (guild.mentionCount
      ? t('lounge.guildRail.mentionsSuffix', { count: guild.mentionCount })
      : guild.unread
        ? t('lounge.guildRail.unreadSuffix')
        : '');
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

interface DndBits {
  props: Record<string, unknown>;
  mark: DropMark;
  dragging: string | null;
}

function GuildIcon({
  guild,
  selected,
  rail,
  dnd,
  inFolder,
}: {
  guild: Guild;
  selected: boolean;
  rail: RailEntry[];
  dnd: DndBits;
  inFolder?: string;
}) {
  const t = useT();
  const inVoice = useVoiceSession((s) => s.guildId === guild.id && s.status !== 'idle');
  const voice = inVoice || guild.voiceConnected;
  const pill: RailPillState = selected ? 'selected' : guild.unread ? 'unread' : 'none';
  const setLayout = useLoungeUi((s) => s.setRailLayout);
  const core = useGuildMenuCore(guild);
  const folders = rail.filter(
    (e): e is Extract<RailEntry, { kind: 'folder' }> => e.kind === 'folder',
  );
  const menu: MenuEntry[] = [
    core.markRead,
    core.invite,
    core.mute,
    core.notifications,
    core.hideMuted,
    { type: 'separator' },
    ...(guild.isStaff ? [core.platformAdmin] : []),
    core.settingsMenu,
    core.privacy,
    core.editProfile,
    core.createChannel,
    core.createCategory,
    { label: t('lounge.guildMenu.createEvent'), disabled: true },
    { type: 'separator' },
    ...(inFolder
      ? [
          {
            label: t('lounge.guildRail.removeFromFolder'),
            onSelect: () => setLayout(removeFromFolder(rail, guild.id)),
          } as MenuEntry,
        ]
      : [
          {
            label: t('lounge.guildRail.moveUp'),
            onSelect: () => setLayout(moveBy(rail, guild.id, -1)),
          } as MenuEntry,
          {
            label: t('lounge.guildRail.moveDown'),
            onSelect: () => setLayout(moveBy(rail, guild.id, 1)),
          } as MenuEntry,
        ]),
    ...(folders.length
      ? [
          {
            type: 'submenu',
            label: t('lounge.guildRail.addToFolder'),
            items: folders.map((f) => ({
              label: f.name ?? t('lounge.guildRail.folder'),
              onSelect: () => setLayout(addToFolder(rail, guild.id, f.id)),
            })),
          } as MenuEntry,
        ]
      : []),
    { type: 'separator' },
    core.leave,
    core.copyId,
  ];
  return (
    <ContextMenu items={menu} label={t('lounge.guildRail.guildMenu', { name: guild.name })}>
      <div
        className="lng-rail-item"
        data-selected={selected || undefined}
        data-muted={guild.muted || undefined}
        data-dragging={dnd.dragging === guild.id || undefined}
        data-drop={dnd.mark?.key === guild.id ? dnd.mark.zone : undefined}
        {...dnd.props}
      >
        <RailPill state={pill} />
        <Tooltip
          content={
            guild.name +
            (guild.isStaff ? t('lounge.platformAdmin.railSuffix') : '') +
            (voice ? t('lounge.guildRail.inVoiceSuffix') : '')
          }
          placement="right"
          delay={150}
        >
          <GuildLink
            guild={guild}
            className={`lng-guild${guild.isStaff ? ' lng-guild-staff' : ''}`}
          >
            {guild.iconUrl ? (
              <img src={guild.iconUrl} alt="" draggable={false} />
            ) : (
              <span aria-hidden="true">{acronym(guild.name)}</span>
            )}
            {voice && (
              <span className="lng-guild-voice" title={t('lounge.guildRail.connectedToVoice')}>
                <Icon name="volume" size={16} />
              </span>
            )}
            {guild.isStaff && (
              <span className="lng-staff-badge" aria-hidden="true">
                {t('lounge.platformAdmin.badge')}
              </span>
            )}
            {guild.isNew && <span className="lng-new-badge">{t('lounge.guildRail.newBadge')}</span>}
          </GuildLink>
        </Tooltip>
        <CounterBadge
          count={guild.mentionCount}
          noun={t('lounge.guildRail.mentionsNoun')}
          className="lng-mention-badge"
        />
      </div>
    </ContextMenu>
  );
}

function Folder({
  entry,
  guilds,
  activeGuildId,
  rail,
  dndFor,
  mark,
  dragging,
}: {
  entry: Extract<RailEntry, { kind: 'folder' }>;
  guilds: Guild[];
  activeGuildId?: string;
  rail: RailEntry[];
  dndFor: (key: string, kind: 'guild' | 'folder', target: DropTarget) => Record<string, unknown>;
  mark: DropMark;
  dragging: string | null;
}) {
  const t = useT();
  const open = useLoungeUi((s) => !!s.openFolders[entry.id]);
  const toggle = useLoungeUi((s) => s.toggleFolder);
  const setLayout = useLoungeUi((s) => s.setRailLayout);
  const openDialog = useLoungeUi((s) => s.openDialog);
  const actions = useLoungeActions();
  const members = entry.guildIds
    .map((id) => guilds.find((g) => g.id === id))
    .filter((g): g is Guild => !!g);
  const unread = members.some((g) => g.unread);
  const mentions = members.reduce((n, g) => n + g.mentionCount, 0);
  const name = entry.name ?? members.map((g) => g.name).join(', ');
  const menu: MenuEntry[] = [
    {
      label: t('lounge.guildRail.markFolderRead'),
      disabled: !unread,
      onSelect: () => members.forEach((g) => g.unread && actions.markGuildRead(g.id)),
    },
    {
      label: t('lounge.guildRail.folderSettings'),
      onSelect: () => openDialog({ kind: 'folder', folderId: entry.id }),
    },
    { type: 'separator' },
    { label: t('lounge.guildRail.moveUp'), onSelect: () => setLayout(moveBy(rail, entry.id, -1)) },
    { label: t('lounge.guildRail.moveDown'), onSelect: () => setLayout(moveBy(rail, entry.id, 1)) },
    {
      label: t('lounge.guildRail.ungroupFolder'),
      danger: true,
      onSelect: () => setLayout(deleteFolder(rail, entry.id)),
    },
  ];
  const dnd = (key: string, target: DropTarget, kind: 'guild' | 'folder') =>
    dndFor(key, kind, target);
  return (
    <li
      className="lng-folder"
      data-open={open || undefined}
      style={{ ['--folder-color' as string]: entry.color }}
    >
      <ContextMenu items={menu} label={t('lounge.guildRail.folderMenu', { name })}>
        <div
          className="lng-rail-item"
          data-dragging={dragging === entry.id || undefined}
          data-drop={mark?.key === entry.id ? mark.zone : undefined}
          {...dnd(entry.id, { kind: 'top', entry }, 'folder')}
        >
          <RailPill state={unread && !open ? 'unread' : 'none'} />
          <Tooltip content={name} placement="right" delay={150}>
            <button
              type="button"
              className="lng-guild lng-folder-btn"
              aria-expanded={open}
              aria-label={`${t('lounge.guildRail.folderLabel', { name })}${mentions ? t('lounge.guildRail.mentionsSuffix', { count: mentions }) : unread ? t('lounge.guildRail.unreadSuffix') : ''}`}
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
          {!open && (
            <CounterBadge
              count={mentions}
              noun={t('lounge.guildRail.mentionsNoun')}
              className="lng-mention-badge"
            />
          )}
        </div>
      </ContextMenu>
      {open && (
        <ul className="lng-folder-list" aria-label={name}>
          {members.map((g) => (
            <li key={g.id}>
              <GuildIcon
                guild={g}
                selected={g.id === activeGuildId}
                rail={rail}
                inFolder={entry.id}
                dnd={{
                  props: dnd(
                    g.id,
                    {
                      kind: 'inFolder',
                      folderId: entry.id,
                      guildId: g.id,
                      siblings: entry.guildIds,
                    },
                    'guild',
                  ),
                  mark,
                  dragging,
                }}
              />
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

export function GuildRail() {
  const t = useT();
  const guilds = useGuilds();
  const rail = useRail();
  useLoadNotifSettings(guilds.map((g) => g.id));
  const { itemProps, mark, dragging } = useRailDnd(rail);
  const dms = useDms();
  const users = useUsers();
  const { guildId } = useLoungeRoute();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const dmMentions = dms.reduce((n, d) => n + d.mentionCount, 0);
  const unreadDms = dms.filter((d) => d.unread);
  const homeSelected = guildId === HOME || guildId === undefined;
  return (
    <RovingNav
      className="lng-rail"
      aria-label={t('lounge.guildRail.servers')}
      selector=".lng-rail-item :is(a[href], button)"
    >
      <ul className="lng-rail-list">
        <li>
          <div className="lng-rail-item" data-selected={homeSelected || undefined}>
            <RailPill state={homeSelected ? 'selected' : unreadDms.length ? 'unread' : 'none'} />
            <Tooltip content={t('lounge.guildRail.directMessages')} placement="right" delay={150}>
              <Link
                to="/channels/$guildId"
                params={{ guildId: HOME }}
                className="lng-guild lng-home"
                aria-label={`${t('lounge.guildRail.directMessages')}${dmMentions ? t('lounge.guildRail.mentionsSuffix', { count: dmMentions }) : ''}`}
              >
                <Icon name="door" size={24} />
              </Link>
            </Tooltip>
            <CounterBadge
              count={dmMentions}
              noun={t('lounge.guildRail.mentionsNoun')}
              className="lng-mention-badge"
            />
          </div>
        </li>
        {unreadDms.map((d) => {
          const u = users[d.recipientIds[0] ?? ''];
          const name = d.name ?? u?.displayName ?? t('lounge.guildRail.directMessage');
          return (
            <li key={d.id}>
              <div className="lng-rail-item">
                <Tooltip content={name} placement="right" delay={150}>
                  <Link
                    to="/channels/$guildId/$channelId"
                    params={{ guildId: HOME, channelId: d.id }}
                    className="lng-guild lng-dm-unread"
                    aria-label={t('lounge.guildRail.unreadDm', { name })}
                  >
                    <Avatar name={name} size={40} />
                  </Link>
                </Tooltip>
                <CounterBadge
                  count={d.mentionCount}
                  noun={t('lounge.guildRail.mentionsNoun')}
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
            return (
              <Folder
                key={e.id}
                entry={e}
                guilds={guilds}
                rail={rail}
                {...(guildId ? { activeGuildId: guildId } : {})}
                mark={mark}
                dragging={dragging}
                dndFor={(key, kind, target) => itemProps(key, { kind, id: key }, target)}
              />
            );
          const g = guilds.find((x) => x.id === e.guildId);
          return g ? (
            <li key={g.id}>
              <GuildIcon
                guild={g}
                selected={g.id === guildId}
                rail={rail}
                dnd={{
                  props: itemProps(g.id, { kind: 'guild', id: g.id }, { kind: 'top', entry: e }),
                  mark,
                  dragging,
                }}
              />
            </li>
          ) : null;
        })}
        <li>
          <div className="lng-rail-item">
            <Tooltip content={t('lounge.guildRail.addServer')} placement="right" delay={150}>
              <button
                type="button"
                className="lng-guild lng-rail-action"
                aria-label={t('lounge.guildRail.addServer')}
                onClick={() => openDialog({ kind: 'addServer' })}
              >
                <Icon name="plus" size={24} />
              </button>
            </Tooltip>
          </div>
        </li>
        <li>
          <div className="lng-rail-item">
            <Tooltip content={t('lounge.guildRail.discover')} placement="right" delay={150}>
              <button
                type="button"
                className="lng-guild lng-rail-action"
                aria-label={t('lounge.guildRail.discover')}
                onClick={() => openDialog({ kind: 'directory' })}
              >
                <Icon name="compass" size={24} />
              </button>
            </Tooltip>
          </div>
        </li>
      </ul>
    </RovingNav>
  );
}
