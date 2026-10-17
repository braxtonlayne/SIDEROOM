import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { cdnUrl } from '../../store/cdn';
import { t } from '../../i18n';

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
  /** The profile effect: the server sends its slug (userset-be); `{ id }` is Discord's object shape. */
  profile_effect?: string | { id: string } | null;
  /** Slug (file stem) of the avatar decoration, as the server sends it in `user_profile`. */
  avatar_decoration?: string | null;
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
    sr_patron?: boolean;
    /** Discord's decoration shape; the server sends `user_profile.avatar_decoration` (a slug) instead. */
    avatar_decoration_data?: { asset: string; sku_id?: string | null } | null;
    /** Contract: short server-tag text. */
    primary_guild?: { tag?: string | null; identity_guild_id?: string | null } | null;
  };
  user_profile: WireProfileFields;
  /** When the account was created ("Member since"); the snowflake time of `user.id` when absent. */
  member_since?: string | null;
  /** Older name of `member_since`, still read for safety. */
  created_at?: string | null;
  /**
   * Platform badges. The server sends ids (`owner`, `staff`, `patron`, `contributor`, `early_member`,
   * `bug_hunter`, `honorary`, `app_developer`, `bot`); objects with a description are also accepted.
   */
  badges?: (WireBadge | string)[] | null;
  connected_accounts: {
    id: string;
    name: string;
    type: string;
    verified: boolean;
    /** Public stats such as `{"followers": 12}` (empty unless the owner shares them). */
    stats?: Record<string, number | string> | string | null;
    /** Link to the account's public page, when known. */
    url?: string | null;
  }[];
  friends_since?: string | null;
  mutual_friends?:
    | { id: string; username: string | null; global_name?: string | null; avatar?: string | null }[]
    | null;
  mutual_friends_count?: number | null;
  mutual_guilds?: { id: string; nick?: string | null; member_since: string }[] | null;
  /** The person's profile inside `guild_id` (only when that query parameter is sent and the server is shared). */
  guild_member_profile?:
    | (WireProfileFields & {
        guild_id: string;
        nick?: string | null;
        avatar?: string | null;
        joined_at?: string | null;
      })
    | null;
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

const STAT_LABELS: Record<string, string> = {
  followers: 'lounge.fullProfile.statFollowers',
  subscribers: 'lounge.fullProfile.statSubscribers',
  karma: 'lounge.fullProfile.statKarma',
  stars: 'lounge.fullProfile.statStars',
  repos: 'lounge.fullProfile.statRepos',
};

/** One line for a connection's public stats: "12 followers · 3 repos"; null when there is nothing to show. */
export function statsText(stats: Profile['connected_accounts'][number]['stats']): string | null {
  if (!stats) return null;
  if (typeof stats === 'string') return stats.trim() || null;
  // The server sends only these five keys (G-53); each has a translated, pluralised label.
  const parts = Object.entries(stats)
    .filter(([, v]) => v !== null && v !== '')
    .map(([k, v]) => {
      const key = STAT_LABELS[k === 'public_repos' ? 'repos' : k];
      if (key && typeof v === 'number') return t(key, { count: v });
      return `${typeof v === 'number' ? v.toLocaleString() : v} ${k.replace(/_/g, ' ')}`;
    });
  return parts.length ? parts.join(' · ') : null;
}
