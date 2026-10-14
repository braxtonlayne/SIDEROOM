import { Avatar, ContextMenu, Icon, VirtualList, type MenuEntry } from '@sideroom/ui';
import { useMemo, useRef } from 'react';
import { useViewer, type Viewer } from '../settings/server/hooks';
import { useCurrentUser, useGuild, useMembers, usePresenceMap, useRoles } from './data';
import { buildMemberRows, type MemberEntry } from './memberGroups';
import { openProfilePopout } from './profile/popoutStore';
import { setMemberRole } from './profile/memberRoles';
import { useOpenDm } from './shell/dm';
import { useUserMenuEntries } from './shell/userMenu';
import { useLoungeUi } from './store';
import { useVoiceMenuEntries } from './voice/useVoiceMenu';

const OFFLINE_HIDE_THRESHOLD = 1000;

function MemberRowView({
  entry,
  guildId,
  ownerId,
  viewer,
}: {
  entry: MemberEntry;
  guildId: string;
  ownerId: string | undefined;
  viewer: Viewer;
}) {
  const rowRef = useRef<HTMLButtonElement | null>(null);
  const { member, presence, color } = entry;
  const u = member.user;
  const name = member.nick ?? u.displayName;
  const me = useCurrentUser();
  const roles = useRoles(guildId);
  const openDm = useOpenDm();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const isSelf = u.id === me.id;
  const userEntries = useUserMenuEntries(isSelf ? undefined : { id: u.id, name });
  // Only people connected to a voice channel get these (volume, mute, and for moderators server mute / move).
  const voiceEntries = useVoiceMenuEntries(guildId, { id: u.id, name });
  const isOwner = ownerId === u.id;
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
      label: 'Profile',
      onSelect: () => openProfilePopout({ userId: u.id, guildId }, rowRef.current),
    },
    { label: 'Mention', onSelect: () => void navigator.clipboard?.writeText(`<@${u.id}>`) },
    {
      label: 'Message',
      disabled: isSelf || !!u.bot,
      onSelect: () => void openDm([u.id]).catch(() => undefined),
    },
    ...(userEntries.note ? [userEntries.note] : []),
    {
      label: 'Change Nickname',
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
            label: 'Roles',
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
      ? [{ label: 'Timeout', onSelect: act('timeout') } as MenuEntry]
      : []),
    ...(viewer.can('KICK_MEMBERS') && canAct
      ? [{ label: `Kick ${name}`, danger: true, onSelect: act('kick') } as MenuEntry]
      : []),
    ...(viewer.can('BAN_MEMBERS') && canAct
      ? [{ label: `Ban ${name}`, danger: true, onSelect: act('ban') } as MenuEntry]
      : []),
    ...(viewer.isOwner && !isSelf && !u.bot
      ? [
          {
            label: 'Transfer Ownership',
            danger: true,
            onSelect: () => openDialog({ kind: 'transferOwnership', guildId, userId: u.id }),
          } as MenuEntry,
        ]
      : []),
    { type: 'separator' },
    { label: 'Copy User ID', onSelect: () => void navigator.clipboard?.writeText(u.id) },
  ];
  return (
    <ContextMenu items={menu} label={`${name} actions`}>
      <button
        ref={rowRef}
        type="button"
        className="lng-member"
        aria-haspopup="dialog"
        onClick={(e) => openProfilePopout({ userId: u.id, guildId }, e.currentTarget)}
        data-offline={presence.status === 'offline' || undefined}
        aria-label={`${name}, ${presence.status}${presence.activity ? `, ${presence.activity.text}` : ''}`}
      >
        <Avatar name={name} src={u.avatarUrl ?? null} size={32} status={presence.status} />
        <span className="lng-member-text">
          <span className="lng-member-name" style={color ? { color } : undefined}>
            {name}
            {ownerId === u.id && <Icon name="star" size={16} label="Server owner" />}
            {u.bot && <span className="lng-bot">APP</span>}
            {u.tag && <span className="lng-tag">{u.tag}</span>}
          </span>
          {presence.activity && <span className="lng-member-sub">{presence.activity.text}</span>}
        </span>
      </button>
    </ContextMenu>
  );
}

export function MemberList({ guildId }: { guildId: string }) {
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
    <VirtualList
      className="lng-members"
      aria-label="Members"
      role="list"
      count={rows.length}
      estimateSize={44}
      getItemKey={(i) => rows[i]?.key ?? i}
      renderItem={(i) => {
        const r = rows[i];
        if (!r) return null;
        return r.kind === 'header' ? (
          <h3 className="lng-member-group">
            {r.label} — {r.count}
          </h3>
        ) : (
          <MemberRowView
            entry={r.entry}
            guildId={guildId}
            ownerId={guild?.ownerId}
            viewer={viewer}
          />
        );
      }}
    />
  );
}
