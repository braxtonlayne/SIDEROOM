import { Avatar, ContextMenu, Icon, VirtualList, type MenuEntry } from '@sideroom/ui';
import { RovingScope } from '../a11y/RovingNav';
import { useMemo, useRef } from 'react';
import { useT } from '../i18n';
import { nameStyle } from '../lib/roleGradient';
import { useDeveloperMode } from '../settings/user/prefs';
import { useViewer, type Viewer } from '../settings/server/hooks';
import { useAppsMenu } from './apps/useAppsMenu';
import { useCurrentUser, useGuild, useMembers, usePresenceMap, useRoles } from './data';
import { buildMemberRows, type MemberEntry } from './memberGroups';
import { openProfilePopout } from './profile/popoutStore';
import { setMemberRole } from './profile/memberRoles';
import { useOpenDm } from './shell/dm';
import { useUserMenuEntries } from './shell/userMenu';
import { useLoungeUi } from './store';
import { useVoiceMenuEntries } from './voice/useVoiceMenu';
import { ServerTagChip } from './components/ServerTagChip';

const OFFLINE_HIDE_THRESHOLD = 1000;

function MemberRowView({
  entry,
  guildId,
  channelId,
  ownerId,
  viewer,
}: {
  entry: MemberEntry;
  guildId: string;
  channelId: string | undefined;
  ownerId: string | undefined;
  viewer: Viewer;
}) {
  const t = useT();
  const developerMode = useDeveloperMode();
  const rowRef = useRef<HTMLButtonElement | null>(null);
  const { member, presence, stops } = entry;
  const u = member.user;
  const name = member.nick ?? u.displayName;
  const me = useCurrentUser();
  const roles = useRoles(guildId);
  const openDm = useOpenDm();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const isSelf = u.id === me.id;
  const userEntries = useUserMenuEntries(isSelf ? undefined : { id: u.id, name }, guildId);
  // Only people connected to a voice channel get these (volume, mute, and for moderators server mute / move).
  const voiceEntries = useVoiceMenuEntries(guildId, { id: u.id, name });
  const isOwner = ownerId === u.id;
  const appsMenu = useAppsMenu(2, { guildId, channelId }, u.id);
  // The server enforces the hierarchy; these only hide what would certainly be refused.
  const targetTop = Math.max(
    -1,
    ...member.roleIds.map((id) => roles.find((r) => r.id === id)?.position ?? -1),
  );
  const outranks = viewer.isOwner || viewer.topPosition > targetTop;
  const canAct = !isSelf && !isOwner && outranks;

  const setRole = (roleId: string, on: boolean) => {
    void setMemberRole(
      guildId,
      { userId: u.id, nick: member.nick, roleIds: member.roleIds },
      roleId,
      on,
    ).catch(() => undefined);
  };
  const assignable = roles.filter((r) => r.id !== guildId && viewer.canManageRole(r.position));

  const act = (action: 'timeout' | 'kick' | 'ban') => () =>
    openDialog({ kind: 'memberAction', action, guildId, userId: u.id });

  const menu: MenuEntry[] = [
    {
      label: t('lounge.memberList.profile'),
      onSelect: () => openProfilePopout({ userId: u.id, guildId }, rowRef.current),
    },
    {
      label: t('lounge.memberList.mention'),
      onSelect: () => void navigator.clipboard?.writeText(`<@${u.id}>`),
    },
    {
      label: t('lounge.memberList.message'),
      disabled: isSelf || !!u.bot,
      onSelect: () => void openDm([u.id]).catch(() => undefined),
    },
    ...(userEntries.note ? [userEntries.note] : []),
    {
      label: t('lounge.memberList.changeNickname'),
      onSelect: () => openDialog({ kind: 'nickname', guildId, userId: u.id }),
    },
    ...(userEntries.invite ? [userEntries.invite] : []),
    ...userEntries.relationship,
    ...(voiceEntries.length ? [{ type: 'separator' } as MenuEntry, ...voiceEntries] : []),
    { type: 'separator' },
    ...(assignable.length > 0 && !isOwner
      ? [
          {
            type: 'submenu',
            label: t('lounge.memberList.roles'),
            items: assignable.map((r): MenuEntry => ({
              type: 'checkbox',
              label: r.name,
              checked: member.roleIds.includes(r.id),
              onCheckedChange: (on) => setRole(r.id, on),
            })),
          } as MenuEntry,
        ]
      : []),
    ...(viewer.can('MODERATE_MEMBERS') && canAct
      ? [{ label: t('lounge.memberList.timeout'), onSelect: act('timeout') } as MenuEntry]
      : []),
    ...(viewer.can('KICK_MEMBERS') && canAct
      ? [
          {
            label: t('lounge.memberList.kick', { name }),
            danger: true,
            onSelect: act('kick'),
          } as MenuEntry,
        ]
      : []),
    ...(viewer.can('BAN_MEMBERS') && canAct
      ? [
          {
            label: t('lounge.memberList.ban', { name }),
            danger: true,
            onSelect: act('ban'),
          } as MenuEntry,
        ]
      : []),
    ...(viewer.isOwner && !isSelf && !u.bot
      ? [
          {
            label: t('lounge.memberList.transferOwnership'),
            danger: true,
            onSelect: () => openDialog({ kind: 'transferOwnership', guildId, userId: u.id }),
          } as MenuEntry,
        ]
      : []),
    ...(appsMenu.length > 0 ? [{ type: 'separator' } as MenuEntry, ...appsMenu] : []),
    ...(developerMode
      ? [
          { type: 'separator' } as MenuEntry,
          {
            label: t('lounge.memberList.copyUserId'),
            onSelect: () => void navigator.clipboard?.writeText(u.id),
          },
        ]
      : []),
  ];
  return (
    <ContextMenu items={menu} label={t('lounge.memberList.actions', { name })}>
      <button
        ref={rowRef}
        type="button"
        className="lng-member"
        aria-haspopup="dialog"
        onClick={(e) => openProfilePopout({ userId: u.id, guildId }, e.currentTarget)}
        data-offline={presence.status === 'offline' || undefined}
        aria-label={
          presence.activity
            ? t('lounge.memberList.rowLabelActivity', {
                name,
                status: presence.status,
                activity: presence.activity.text,
              })
            : t('lounge.memberList.rowLabel', { name, status: presence.status })
        }
      >
        <Avatar
          name={name}
          src={u.avatarUrl ?? null}
          size={32}
          status={presence.status}
          ring={u.patron}
        />
        <span className="lng-member-text">
          <span className="lng-member-name" style={stops[0] ? { color: stops[0] } : undefined}>
            {stops.length > 1 ? (
              <span className="lng-member-gradient" data-gradient="" style={nameStyle(stops)}>
                {name}
              </span>
            ) : (
              name
            )}
            {ownerId === u.id && (
              <Icon name="star" size={16} label={t('lounge.memberList.serverOwner')} />
            )}
            {u.bot && <span className="lng-bot">{t('lounge.memberList.app')}</span>}
            <ServerTagChip user={u} />
          </span>
          {presence.activity && <span className="lng-member-sub">{presence.activity.text}</span>}
        </span>
      </button>
    </ContextMenu>
  );
}

export function MemberList({
  guildId,
  channelId,
}: {
  guildId: string;
  channelId?: string | undefined;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const presence = usePresenceMap();
  const viewer = useViewer(guildId);
  const rows = useMemo(
    () =>
      buildMemberRows(members, roles, presence, {
        hideOffline: members.length > OFFLINE_HIDE_THRESHOLD,
      }),
    [members, roles, presence],
  );
  return (
    <RovingScope selector=".lng-member">
      <VirtualList
        className="lng-members"
        aria-label={t('lounge.memberList.members')}
        role="list"
        count={rows.length}
        estimateSize={44}
        getItemKey={(i) => rows[i]?.key ?? i}
        renderItem={(i) => {
          const r = rows[i];
          if (!r) return null;
          return r.kind === 'header' ? (
            <h3 className="lng-member-group">
              {t('lounge.memberList.groupHeader', { label: r.label, count: r.count })}
            </h3>
          ) : (
            <MemberRowView
              entry={r.entry}
              guildId={guildId}
              channelId={channelId}
              ownerId={guild?.ownerId}
              viewer={viewer}
            />
          );
        }}
      />
    </RovingScope>
  );
}
