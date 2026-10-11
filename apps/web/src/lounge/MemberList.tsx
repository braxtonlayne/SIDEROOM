import { Avatar, ContextMenu, Icon, VirtualList, type MenuEntry } from '@sideroom/ui';
import { useMemo } from 'react';
import { useGuild, useMembers, usePresenceMap, useRoles } from './data';
import { buildMemberRows, type MemberEntry } from './memberGroups';

const OFFLINE_HIDE_THRESHOLD = 1000;

function MemberRowView({ entry, ownerId }: { entry: MemberEntry; ownerId: string | undefined }) {
  const { member, presence, color } = entry;
  const u = member.user;
  const name = member.nick ?? u.displayName;
  const menu: MenuEntry[] = [
    { label: 'Profile' },
    { label: 'Mention', onSelect: () => void navigator.clipboard?.writeText(`<@${u.id}>`) },
    { label: 'Message' },
    { type: 'separator' },
    { label: 'Copy User ID', onSelect: () => void navigator.clipboard?.writeText(u.id) },
  ];
  return (
    <ContextMenu items={menu} label={`${name} actions`}>
      <button
        type="button"
        className="lng-member"
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
          <MemberRowView entry={r.entry} ownerId={guild?.ownerId} />
        );
      }}
    />
  );
}
