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
  | { kind: 'member'; key: string; entry: MemberEntry };

const byName = (a: MemberEntry, b: MemberEntry) =>
  (a.member.nick ?? a.member.user.displayName).localeCompare(
    b.member.nick ?? b.member.user.displayName,
  );

/**
 * Discord grouping: one group per hoisted role (highest role wins), then Online, then Offline.
 * Offline members never appear under a role. Large servers hide Offline (`hideOffline`).
 */
export function buildMemberRows(
  members: readonly Member[],
  roles: readonly Role[],
  presence: Record<Snowflake, Presence>,
  opts: { hideOffline?: boolean } = {},
): MemberRow[] {
  const roleById = new Map(roles.map((r) => [r.id, r]));
  const hoisted = roles.filter((r) => r.hoist).sort((a, b) => b.position - a.position);
  const groups = new Map<string, MemberEntry[]>(hoisted.map((r) => [r.id, []]));
  const online: MemberEntry[] = [];
  const offline: MemberEntry[] = [];

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
      offline.push(entry);
      continue;
    }
    const top = own.find((r) => r.hoist);
    if (top) groups.get(top.id)?.push(entry);
    else online.push(entry);
  }

  const rows: MemberRow[] = [];
  const emit = (key: string, label: string, list: MemberEntry[]) => {
    if (!list.length) return;
    rows.push({ kind: 'header', key: `h:${key}`, label, count: list.length });
    for (const entry of list.sort(byName))
      rows.push({ kind: 'member', key: `${key}:${entry.member.user.id}`, entry });
  };
  for (const r of hoisted) emit(r.id, r.name, groups.get(r.id) ?? []);
  emit('online', 'Online', online);
  if (!opts.hideOffline) emit('offline', 'Offline', offline);
  return rows;
}
