import type { BridgedMember } from '../settings/server/bridge/api';
import type { Member, Presence, Role, Snowflake } from './data';

export interface MemberEntry {
  member: Member;
  presence: Presence;
  /** Highest colored role color. */
  color: string | null;
  /** All gradient stops of that role, first stop first (one entry for a plain colour; J-08). */
  stops: string[];
}

export type MemberRow =
  | { kind: 'header'; key: string; label: string; count: number }
  | { kind: 'member'; key: string; entry: MemberEntry }
  /** A Discord member of a whole-server bridge, listed among the others by their mapped roles (display only). */
  | { kind: 'bridged'; key: string; member: BridgedMember; color: string | null };

/**
 * Discord grouping: one group per hoisted role (highest role wins), then Online, then Offline.
 * Offline members never appear under a role. Large servers hide Offline (`hideOffline`).
 */
export function buildMemberRows(
  members: readonly Member[],
  roles: readonly Role[],
  presence: Record<Snowflake, Presence>,
  opts: {
    hideOffline?: boolean;
    /** Whole-server bridge: Discord members with `role_ids`, grouped under the same hoisted roles (35b). */
    bridged?: readonly BridgedMember[];
  } = {},
): MemberRow[] {
  const roleById = new Map(roles.map((r) => [r.id, r]));
  const hoisted = roles.filter((r) => r.hoist).sort((a, b) => b.position - a.position);
  type Item = { name: string; row: (key: string) => MemberRow };
  const groups = new Map<string, Item[]>(hoisted.map((r) => [r.id, []]));
  const online: Item[] = [];
  const offline: Item[] = [];
  const itemOf = (entry: MemberEntry): Item => ({
    name: entry.member.nick ?? entry.member.user.displayName,
    row: (key) => ({ kind: 'member', key: `${key}:${entry.member.user.id}`, entry }),
  });

  for (const member of members) {
    const p = presence[member.user.id] ?? { status: 'offline' as const };
    const own = member.roleIds
      .map((id) => roleById.get(id))
      .filter((r): r is Role => !!r)
      .sort((a, b) => b.position - a.position);
    const coloured = own.find((r) => r.color);
    const color = coloured?.color ?? null;
    const stops = color
      ? [
          color,
          ...(coloured?.colorSecondary
            ? [coloured.colorSecondary, ...(coloured.colorTertiary ? [coloured.colorTertiary] : [])]
            : []),
        ]
      : [];
    const entry: MemberEntry = { member, presence: p, color, stops };
    if (p.status === 'offline') {
      offline.push(itemOf(entry));
      continue;
    }
    const top = own.find((r) => r.hoist);
    if (top) groups.get(top.id)?.push(itemOf(entry));
    else online.push(itemOf(entry));
  }

  // Discord members read like the rest: the same hoisted groups, by the roles their Discord roles map to. Unknown
  // presence is not offline (the bot may lack the presence intent), so it is listed with the others.
  for (const b of opts.bridged ?? []) {
    const own = (b.role_ids ?? [])
      .map((id) => roleById.get(id))
      .filter((r): r is Role => !!r)
      .sort((a, b2) => b2.position - a.position);
    const color = own.find((r) => r.color)?.color ?? null;
    const item: Item = {
      name: b.display_name,
      row: (key) => ({ kind: 'bridged', key: `${key}:b${b.id}`, member: b, color }),
    };
    if (b.presence === 'offline') {
      offline.push(item);
      continue;
    }
    const top = own.find((r) => r.hoist);
    if (top) groups.get(top.id)?.push(item);
    else online.push(item);
  }

  const rows: MemberRow[] = [];
  const emit = (key: string, label: string, list: Item[]) => {
    if (!list.length) return;
    rows.push({ kind: 'header', key: `h:${key}`, label, count: list.length });
    for (const item of list.sort((x, y) => x.name.localeCompare(y.name))) rows.push(item.row(key));
  };
  for (const r of hoisted) emit(r.id, r.name, groups.get(r.id) ?? []);
  emit('online', 'Online', online);
  if (!opts.hideOffline) emit('offline', 'Offline', offline);
  return rows;
}
