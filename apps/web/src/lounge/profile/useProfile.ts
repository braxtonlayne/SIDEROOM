import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { cdnUrl } from '../../store/cdn';

/** One badge on the profile: `id` is the stable key, `description` the tooltip (Discord's shape). */
export interface WireBadge {
  id: string;
  description?: string | null;
  icon?: string | null;
}

/** Profile fields a person can set, as sent for the account (`user_profile`) and for one server (`guild_member_profile`). */
export interface WireProfileFields {
  bio?: string | null;
  pronouns?: string | null;
  banner?: string | null;
  accent_color?: number | null;
  /** Two RGB integers: the start and end of the profile gradient (Discord's `theme_colors`). */
  theme_colors?: [number, number] | null;
  /** An approved free effect pack entry; `id` names the effect. */
  profile_effect?: { id: string } | null;
}

/**
 * `GET /users/{id}/profile?with_mutual_guilds=true&guild_id=` (ProfileObject in the OpenAPI spec).
 *
 * Fields marked "contract" are not in the backend yet (docs/05 describes Discord's shape, which they follow);
 * the client treats them as optional and renders nothing when they are absent.
 */
export interface Profile {
  user: {
    id: string;
    username: string | null;
    global_name?: string | null;
    avatar?: string | null;
    banner?: string | null;
    bot?: boolean;
    /** Contract: a free decoration from an approved pack, `asset` is the file stem. */
    avatar_decoration_data?: { asset: string; sku_id?: string | null } | null;
    /** Contract: short server-tag text. */
    primary_guild?: { tag?: string | null; identity_guild_id?: string | null } | null;
  };
  user_profile: WireProfileFields;
  /** Contract: the account created time is the snowflake of `user.id` when absent. */
  created_at?: string | null;
  /** Contract: platform badges (staff, patron tiers, contributor tiers, early member, app developer). */
  badges?: WireBadge[] | null;
  connected_accounts: {
    id: string;
    name: string;
    type: string;
    verified: boolean;
    /** Contract: a one-line stats text such as "42 repositories". */
    stats?: string | null;
  }[];
  friends_since?: string | null;
  mutual_friends?:
    | { id: string; username: string | null; global_name?: string | null; avatar?: string | null }[]
    | null;
  mutual_friends_count?: number | null;
  mutual_guilds?: { id: string; nick?: string | null; member_since: string }[] | null;
  /** Contract: the person's profile inside `guild_id` (only when that query parameter is sent). */
  guild_member_profile?:
    (WireProfileFields & { guild_id: string; nick?: string | null; avatar?: string | null }) | null;
}

type State = { kind: 'loading' } | { kind: 'ready'; profile: Profile } | { kind: 'error' };

const cache = new Map<string, Promise<Profile>>();
/** Test hook. */
export const resetProfiles = () => cache.clear();

export function loadProfile(userId: string, guildId?: string): Promise<Profile> {
  const key = guildId ? `${userId}@${guildId}` : userId;
  let p = cache.get(key);
  if (!p) {
    const guild = guildId ? `&guild_id=${encodeURIComponent(guildId)}` : '';
    p = apiFetch<Profile>(
      `/users/${encodeURIComponent(userId)}/profile?with_mutual_guilds=true${guild}`,
      { retries: 0 },
    ).catch((e) => {
      cache.delete(key);
      throw e;
    });
    cache.set(key, p);
  }
  return p;
}

/** The profile of one person (as seen from `guildId` when given), fetched once per session and shared. */
export function useProfile(userId: string, guildId?: string): State {
  const [state, setState] = useState<State>({ kind: 'loading' });
  useEffect(() => {
    let live = true;
    setState({ kind: 'loading' });
    loadProfile(userId, guildId).then(
      (profile) => live && setState({ kind: 'ready', profile }),
      () => live && setState({ kind: 'error' }),
    );
    return () => {
      live = false;
    };
  }, [userId, guildId]);
  return state;
}

/** CSS colour of a profile accent (integer RGB), or undefined. */
export function accentCss(n: number | null | undefined): string | undefined {
  return typeof n === 'number' && n > 0 ? `#${n.toString(16).padStart(6, '0')}` : undefined;
}

export const avatarUrlOf = (p: Profile['user']): string | null =>
  cdnUrl('avatars', p.id, p.avatar, { size: 128 });
export const bannerUrlOf = (p: Profile): string | null =>
  cdnUrl('avatars', p.user.id, p.user_profile.banner ?? p.user.banner, { size: 600 });
