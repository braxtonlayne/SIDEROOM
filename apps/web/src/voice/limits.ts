/**
 * What the viewer may send in a server (VOX-CLI-006/007, VOX-UI-006): the video profiles of their own account
 * (`GET /donations/me`, Patron or free) merged with the server's level perks (`GET /guilds/{id}/support`).
 * Cached for a few minutes so opening the picker is instant; a failed request leaves the free tier.
 */
import { useEffect, useState } from 'react';
import { getGuildSupport, getMe } from '../support/api';
import { allowedProfiles, FREE_PROFILES, type VideoProfile } from './video';

export interface VideoLimits {
  profiles: VideoProfile[];
  /** The member is a Patron (their video does not count against the sender allowance). */
  patron: boolean;
  /** Server support level; 0 when unknown. */
  level: number;
  /** Camera and screen senders the server allows per voice channel; 0 when unknown. */
  senders: number;
  /** "Go Live" audience cap; 0 when unknown. */
  viewers: number;
  loaded: boolean;
}

export const FREE_LIMITS: VideoLimits = {
  profiles: [...FREE_PROFILES],
  patron: false,
  level: 0,
  senders: 0,
  viewers: 0,
  loaded: false,
};

const TTL_MS = 5 * 60_000;
const cache = new Map<string, { at: number; value: VideoLimits }>();

export const clearVideoLimitsCache = () => cache.clear();

export async function fetchVideoLimits(guildId: string | null): Promise<VideoLimits> {
  const key = guildId ?? '@me';
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.value;
  const [me, support] = await Promise.all([
    getMe().catch(() => null),
    guildId && guildId !== '@me' ? getGuildSupport(guildId).catch(() => null) : null,
  ]);
  const value: VideoLimits = {
    profiles: allowedProfiles(me?.limits.video_profiles, support?.perks.video_profiles),
    patron: !!me?.limits.patron,
    level: support?.level ?? 0,
    senders: support?.perks.video_senders ?? 0,
    viewers: Math.max(support?.perks.stream_viewers ?? 0, me?.limits.stream_viewers ?? 0),
    loaded: true,
  };
  if (me || support) cache.set(key, { at: Date.now(), value });
  return value;
}

export function useVideoLimits(guildId: string | null | undefined): VideoLimits {
  const key = guildId ?? '@me';
  const [limits, setLimits] = useState<VideoLimits>(() => cache.get(key)?.value ?? FREE_LIMITS);
  useEffect(() => {
    let live = true;
    void fetchVideoLimits(key === '@me' ? null : key).then((v) => {
      if (live) setLimits(v);
    });
    return () => {
      live = false;
    };
  }, [key]);
  return limits;
}
