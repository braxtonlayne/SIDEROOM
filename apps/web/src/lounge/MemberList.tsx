import { Avatar, ContextMenu, Icon, VirtualList, type MenuEntry } from '@sideroom/ui';
import { RovingScope } from '../a11y/RovingNav';
import { useMemo, useRef, useState } from 'react';
import { useT } from '../i18n';
import { nameStyle } from '../lib/roleGradient';
import { useDeveloperMode } from '../settings/user/prefs';
import { useViewAsMembers } from '../settings/server/viewAsMode';
import { useViewer, type Viewer } from '../settings/server/hooks';
import { useAppsMenu } from './apps/useAppsMenu';
import {
  useCurrentUser,
  useGuild,
  useListLoadFailure,
  useMembers,
  usePresenceMap,
  useRoles,
} from './data';
import { ListLoadFailed } from './ListLoadFailed';
import { buildMemberRows, type MemberEntry } from './memberGroups';
import { openProfilePopout } from './profile/popoutStore';
import { setMemberRole } from './profile/memberRoles';
import { startDmCall } from './shell/dmCall';
import { useOpenDm } from './shell/dm';
import { useSocial } from './shell/social';
import { useUserMenuEntries } from './shell/userMenu';
import { useLoungeUi } from './store';
import { useVoiceMenuEntries } from './voice/useVoiceMenu';
import { ServerTagChip } from './components/ServerTagChip';
import { BridgedMemberRow } from './bridge/BridgedMemberViews';
import { useGuildBridgedMembers } from './bridge/members';
import { failWith } from './actionError';

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
  const blocked = useSocial((st) => st.relationships.some((r) => r.id === u.id && r.type === 2));
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
    ).catch(failWith(on ? 'lounge.actionError.addRole' : 'lounge.actionError.removeRole'));
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
      onSelect: () => void openDm([u.id]).catch(failWith('lounge.actionError.openDm')),
    },
    // LNG-GLD-023: what the DM header's call button does (open or create the DM, ring); not for oneself, bots or blocked people.
    ...(isSelf || u.bot || blocked
      ? []
      : [
          {
            label: t('lounge.memberList.call'),
            onSelect: () =>
              void openDm([u.id])
                .then((dm) => startDmCall(dm.id, name, false))
                .catch(failWith('lounge.actionError.openDm')),
          } as MenuEntry,
        ]),
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
  const members = useViewAsMembers(guildId, channelId, guild?.ownerId, useMembers(guildId));
  const roles = useRoles(guildId);
  const presence = usePresenceMap();
  const viewer = useViewer(guildId);
  const allBridged = useGuildBridgedMembers(guildId);
  // Whole-server bridge: Discord members come with their mapped roles and are listed among the others (35b).
  const native = useMemo(() => allBridged.filter((b) => b.role_ids !== undefined), [allBridged]);
  const bridged = useMemo(() => allBridged.filter((b) => b.role_ids === undefined), [allBridged]);
  const rows = useMemo(
    () =>
      buildMemberRows(members, roles, presence, {
        hideOffline: members.length + native.length > OFFLINE_HIDE_THRESHOLD,
        bridged: native,
      }),
    [members, roles, presence, native],
  );
  // The Discord half of a selective bridge, after the people who are here (owner direction).
  const [needle, setNeedle] = useState('');
  const shownBridged = useMemo(() => {
    const q = needle.trim().toLowerCase();
    return q ? bridged.filter((b) => b.display_name.toLowerCase().includes(q)) : bridged;
  }, [bridged, needle]);
  const total = rows.length;
  const extra = bridged.length === 0 ? 0 : 2 + shownBridged.length;
  // SEC-AUD-046 (D-577): a failed load leads the list (the people known so far, often only you, stay below it).
  const load = useListLoadFailure('members', guildId);
  const lead = load.failed ? 1 : 0;
  return (
    <RovingScope selector=".lng-member">
      <VirtualList
        className="lng-members"
        aria-label={t('lounge.memberList.members')}
        role="list"
        count={lead + total + extra}
        estimateSize={44}
        getItemKey={(at) => {
          if (at < lead) return 'load-failed';
          const i = at - lead;
          return i < total
            ? (rows[i]?.key ?? i)
            : i === total
              ? 'bridged:h'
              : i === total + 1
                ? 'bridged:s'
                : 'bridged:' + String(shownBridged[i - total - 2]?.id);
        }}
        renderItem={(at) => {
          if (at < lead)
            return <ListLoadFailed message={t('lounge.listLoad.members')} onRetry={load.retry} />;
          const i = at - lead;
          if (i >= total) {
            if (i === total)
              return (
                <h3 className="lng-member-group">
                  {t('lounge.memberList.groupHeader', {
                    label: t('lounge.bridge.groupLabel'),
                    count: bridged.length,
                  })}
                </h3>
              );
            if (i === total + 1)
              return (
                <input
                  type="search"
                  className="lng-pop-input lng-bridged-search"
                  aria-label={t('lounge.bridge.searchMembers')}
                  placeholder={t('lounge.bridge.searchMembers')}
                  value={needle}
                  onChange={(e) => setNeedle(e.target.value)}
                />
              );
            const b = shownBridged[i - total - 2];
            return b ? <BridgedMemberRow member={b} guildId={guildId} /> : null;
          }
          const r = rows[i];
          if (!r) return null;
          if (r.kind === 'bridged')
            return <BridgedMemberRow member={r.member} guildId={guildId} color={r.color} />;
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
