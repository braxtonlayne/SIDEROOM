import { apiFetch } from '../../api/client';

/**
 * ACT-EMU-002 owner default: a co-op seat goes only to members of the call's server who hold `view_games` there,
 * the same rule the server applies to the ROM shelves (`sr_storage::scopes::grants_for`): the server owner and
 * ADMINISTRATOR hold every grant, MANAGE_GUILD implies `manage`, `manage` reads games, otherwise a role of the
 * member must carry `view_games`. Guests never receive the ROM either way (they watch the host's screen share);
 * this decides only who may press buttons. The check runs on the host's machine and fails closed.
 */
const ADMINISTRATOR = 1n << 3n;
const MANAGE_GUILD = 1n << 5n;

export interface GateRole {
  id: string;
  permissions: string;
  sr_storage_grants?: string[];
}

export interface GateMember {
  roles: string[];
  /** Membership screening not passed yet: no grants (the server skips pending members too). */
  pending?: boolean;
}

const bits = (s: string): bigint => {
  try {
    return BigInt(s);
  } catch {
    return 0n;
  }
};

/** Pure rule, mirrored from the server (see the file comment). */
export function holdsViewGames(
  guildId: string,
  ownerId: string | undefined,
  userId: string,
  roles: readonly GateRole[],
  memberRoleIds: readonly string[],
): boolean {
  if (ownerId !== undefined && ownerId === userId) return true;
  const byId = new Map(roles.map((r) => [r.id, r]));
  // @everyone (id = guild id) applies to every member, then the member's own roles.
  const held = [guildId, ...memberRoleIds]
    .map((id) => byId.get(id))
    .filter((r): r is GateRole => !!r);
  let perms = 0n;
  for (const r of held) perms |= bits(r.permissions);
  if ((perms & ADMINISTRATOR) !== 0n || (perms & MANAGE_GUILD) !== 0n) return true;
  return held.some((r) => {
    const g = r.sr_storage_grants ?? [];
    return g.includes('view_games') || g.includes('manage');
  });
}

/**
 * The host's `mayTakeSeat` for a call in `guildId`: reads the role list (which carries `sr_storage_grants`) and the
 * guest's member object at each JOIN (JOINs are rare; a grant changed mid-session counts at the next one). Any error
 * refuses the seat. A call outside a server (none
 * today: "Play together" is offered in server calls only) refuses everyone.
 */
export function viewGamesGate(
  guildId: string | null,
  ownerId: string | undefined,
  fetcher: <T>(path: string) => Promise<T> = (p) => apiFetch(p, { retries: 0 }),
): (userId: string) => Promise<boolean> {
  return async (userId) => {
    if (!guildId) return false;
    if (ownerId !== undefined && ownerId === userId) return true;
    try {
      const [list, member] = await Promise.all([
        fetcher<GateRole[]>(`/guilds/${guildId}/roles`),
        fetcher<GateMember>(`/guilds/${guildId}/members/${userId}`),
      ]);
      if (member.pending) return false;
      return holdsViewGames(guildId, ownerId, userId, list, member.roles ?? []);
    } catch {
      return false;
    }
  };
}
