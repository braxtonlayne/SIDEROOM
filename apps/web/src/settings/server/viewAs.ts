// View Server As Role (J-09, LNG-SSET-036): which channels a member holding only the chosen roles would see.
// The computation is the shared `sr-permissions` engine (the one the composer uses, `lounge/permEngine`): the
// roles' permissions, the channel overwrites of those roles and `@everyone`, the `ADMINISTRATOR` bypass.
import type { PermEngine } from '../../lounge/permEngine';
import type { ChannelObject, RoleObject } from './api';
import { PERM, toBig } from './permissions';

/** An id no real account has; the stand-in member matches no member overwrite. */
const STAND_IN = '9223372036854775807';
const CATEGORY = 4;

export interface ViewAsInput {
  guildId: string;
  /** Every role of the server (the `@everyone` role has the guild's id). */
  roles: readonly RoleObject[];
  /** The roles to look through; `@everyone` is always included. */
  picked: ReadonlySet<string>;
  channels: readonly ChannelObject[];
}

export interface ViewAsEntry {
  channel: ChannelObject;
  /** The chosen roles may view the channel (a category: it is shown because one of its channels is). */
  visible: boolean;
}

/** Whether the stand-in member can view `channel`; undefined when the engine rejects the input. */
export function canViewAs(
  engine: PermEngine,
  i: Pick<ViewAsInput, 'guildId' | 'roles' | 'picked'>,
  channel: ChannelObject,
): boolean | undefined {
  const ids = [...i.picked].filter((id) => id !== i.guildId);
  const byId = new Map(i.roles.map((r) => [r.id, r]));
  try {
    const bits = BigInt(
      engine.compute({
        is_owner: false,
        everyone: byId.get(i.guildId)?.permissions ?? '0',
        role_permissions: ids.map((id) => byId.get(id)?.permissions ?? '0'),
        guild_id: i.guildId,
        member_id: STAND_IN,
        member_role_ids: ids,
        overwrites: channel.permission_overwrites.map((o) => ({
          id: o.id,
          kind: o.type === 1 ? 'member' : 'role',
          allow: o.allow,
          deny: o.deny,
        })),
        communication_disabled_until_ms: null,
      }),
    );
    return (bits & PERM.VIEW_CHANNEL!) === PERM.VIEW_CHANNEL;
  } catch {
    return undefined;
  }
}

/**
 * The channel list as the chosen roles would see it, in the order of the real list: categories by position, each
 * followed by its channels, then the channels without a category first. A category appears when one of its
 * channels is visible (or, if it has none, when the category itself is viewable).
 */
export function viewServerAs(engine: PermEngine, i: ViewAsInput): ViewAsEntry[] | undefined {
  const bySort = (a: ChannelObject, b: ChannelObject) =>
    a.position - b.position || a.id.localeCompare(b.id);
  const categories = i.channels.filter((c) => c.type === CATEGORY).sort(bySort);
  const catIds = new Set(categories.map((c) => c.id));
  const children = (id: string) =>
    i.channels.filter((c) => c.type !== CATEGORY && c.parent_id === id).sort(bySort);
  const loose = i.channels
    .filter((c) => c.type !== CATEGORY && (!c.parent_id || !catIds.has(c.parent_id)))
    .sort(bySort);
  const out: ViewAsEntry[] = [];
  const add = (c: ChannelObject): boolean | undefined => {
    const visible = canViewAs(engine, i, c);
    if (visible === undefined) return undefined;
    out.push({ channel: c, visible });
    return visible;
  };
  for (const c of loose) if (add(c) === undefined) return undefined;
  for (const cat of categories) {
    const kids = children(cat.id);
    const catEntry: ViewAsEntry = { channel: cat, visible: false };
    out.push(catEntry);
    let any = false;
    for (const k of kids) {
      const v = add(k);
      if (v === undefined) return undefined;
      any ||= v;
    }
    const own = canViewAs(engine, i, cat);
    if (own === undefined) return undefined;
    catEntry.visible = kids.length ? any : own;
  }
  return out;
}

/** True when the chosen roles hold `ADMINISTRATOR`, which sees every channel. */
export const hasAdministrator = (roles: readonly RoleObject[], picked: ReadonlySet<string>) =>
  roles.some(
    (r) => picked.has(r.id) && (toBig(r.permissions) & PERM.ADMINISTRATOR!) === PERM.ADMINISTRATOR,
  );
