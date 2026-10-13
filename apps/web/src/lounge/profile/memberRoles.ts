import { addMemberRole, removeMemberRole } from '../../settings/server/api';
import { useStore } from '../../store';

/**
 * Adds or removes one role of a member, then tells the store (the gateway repeats it for everyone else).
 * The server checks the role hierarchy; callers only hide what would certainly be refused.
 */
export async function setMemberRole(
  guildId: string,
  member: { userId: string; nick?: string | null | undefined; roleIds: readonly string[] },
  roleId: string,
  on: boolean,
): Promise<void> {
  await (on
    ? addMemberRole(guildId, member.userId, roleId)
    : removeMemberRole(guildId, member.userId, roleId));
  useStore.getState().applyDispatch('GUILD_MEMBER_UPDATE', {
    guild_id: guildId,
    user: { id: member.userId },
    nick: member.nick ?? null,
    roles: on ? [...member.roleIds, roleId] : member.roleIds.filter((r) => r !== roleId),
  });
}
