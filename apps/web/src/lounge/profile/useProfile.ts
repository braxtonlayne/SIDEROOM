import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { cdnUrl } from '../../store/cdn';

/** `GET /users/{id}/profile` (ProfileObject in the OpenAPI spec). */
export interface Profile {
  user: {
    id: string;
    username: string | null;
    global_name?: string | null;
    avatar?: string | null;
    banner?: string | null;
  };
  user_profile: {
    bio?: string | null;
    pronouns?: string | null;
    banner?: string | null;
    accent_color?: number | null;
  };
  connected_accounts: { id: string; name: string; type: string; verified: boolean }[];
  friends_since?: string | null;
  mutual_friends?: { id: string; username: string | null; global_name?: string | null }[] | null;
  mutual_friends_count?: number | null;
  mutual_guilds?: { id: string; nick?: string | null; member_since: string }[] | null;
}

type State = { kind: 'loading' } | { kind: 'ready'; profile: Profile } | { kind: 'error' };

const cache = new Map<string, Promise<Profile>>();
/** Test hook. */
export const resetProfiles = () => cache.clear();

export function loadProfile(userId: string): Promise<Profile> {
  let p = cache.get(userId);
  if (!p) {
    p = apiFetch<Profile>(`/users/${encodeURIComponent(userId)}/profile?with_mutual_guilds=true`, {
      retries: 0,
    }).catch((e) => {
      cache.delete(userId);
      throw e;
    });
    cache.set(userId, p);
  }
  return p;
}

/** The profile of one person, fetched once per session and shared by the popout and the full modal. */
export function useProfile(userId: string): State {
  const [state, setState] = useState<State>({ kind: 'loading' });
  useEffect(() => {
    let live = true;
    setState({ kind: 'loading' });
    loadProfile(userId).then(
      (profile) => live && setState({ kind: 'ready', profile }),
      () => live && setState({ kind: 'error' }),
    );
    return () => {
      live = false;
    };
  }, [userId]);
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
