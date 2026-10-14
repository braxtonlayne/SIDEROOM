// `/cinema/tv/watch/$channel` (TV-PLY): tunes the channel (through the shared age gate), starts the player session
// with live semantics and renders the live view. Changing channel keeps the view mounted (only the tuning changes),
// so the player never flashes to the mini player in between. Coming back from the mini player re-attaches the
// running session instead of tuning again.
import { useQuery } from '@tanstack/react-query';
import { useParams, useRouter } from '@tanstack/react-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { t, useT } from '../../i18n';
import { isAgeGateError, useAgeGate, withAgeGate } from '../../lib/ageGate';
import { LIVE_MEDIA_PREFIX } from '../player/model';
import { playerSession, usePlayer } from '../player/session';
import type { PlaybackInfo } from '../player/types';
import { OffAirError, isNotAvailable, tuneChannel } from './api';
import { LiveView } from './LiveView';
import { tvHref, watchChannelHref } from './model';
import type { TuneGrant } from './types';

/** The session's view of a tuned channel: no duration, no progress, the mini player links back by channel id. */
export function liveInfo(g: TuneGrant): PlaybackInfo {
  const kind = g.now?.kind;
  return {
    title_id: `${LIVE_MEDIA_PREFIX}${g.channel.id}`,
    episode_id: null,
    media_id: `${LIVE_MEDIA_PREFIX}${g.channel.id}`,
    title: g.now?.title ?? g.channel.name,
    episode_label: t('cinema.liveWatch.channelLabel', {
      number: g.channel.number,
      callSign: g.channel.call_sign,
    }),
    kind: kind === 'movie' || kind === 'anime' ? kind : 'series',
    poster_url: g.now?.poster_url ?? null,
    age_restricted: g.channel.age_restricted,
    duration_s: 0,
    master_url: g.master_url,
    expires_at_ms: g.expires_at_ms,
    refresh_at_ms: g.refresh_at_ms ?? null,
    subtitle_tracks: [],
    markers: [],
    sprites_url: null,
    next: null,
    progress: null,
    live: { channel_id: g.channel.id },
  };
}

/** Browsers allow sound without a click only after the page has had user activation. */
const hasActivation = () =>
  typeof navigator !== 'undefined' &&
  (navigator as Navigator & { userActivation?: { hasBeenActive: boolean } }).userActivation
    ?.hasBeenActive === true;

export function LiveWatchRoute() {
  const { channel } = useParams({ strict: false }) as { channel?: string };
  if (!channel) return null;
  return <LiveWatchPage channelId={String(channel)} />;
}

export function LiveWatchPage({ channelId }: { channelId: string }) {
  const tr = useT();
  const router = useRouter();
  const { ensureAdult } = useAgeGate();
  const s = playerSession();
  const [reuseId, setReuseId] = useState<string | null>(() =>
    s.state.status !== 'idle' && s.state.info?.live?.channel_id === channelId ? channelId : null,
  );
  const reuse = reuseId === channelId;
  const [attempt, setAttempt] = useState(0);
  const status = usePlayer((x) => x.status);
  const playing = usePlayer((x) => x.playing);
  // Once the viewer has played, later tunings (channel change, reconnect) continue by themselves.
  const played = useRef(false);
  if (playing) played.current = true;

  const query = useQuery({
    queryKey: ['cinema', 'tv', 'tune', channelId, attempt],
    // 403 900011 -> "Are you 18 or older?" -> on yes the request runs once more (D-184).
    queryFn: () => withAgeGate(() => tuneChannel(channelId), ensureAdult),
    enabled: !reuse,
    retry: false,
    // Signed URLs: never serve a cached copy.
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  const grant = query.data ?? null;

  useEffect(() => {
    if (!grant || reuse) return;
    void s.start(liveInfo(grant), { autoplay: hasActivation() || played.current });
    // A new grant object means a new start (channel change, reconnect).
  }, [grant, reuse, s]);

  const retune = useCallback(() => {
    setReuseId(null);
    setAttempt((n) => n + 1);
  }, []);

  const exit = useCallback(() => {
    if (router.history.canGoBack()) router.history.back();
    else router.history.push(tvHref);
  }, [router]);

  const declined = isAgeGateError(query.error);
  useEffect(() => {
    // The player stays closed (D-184): back to the guide.
    if (declined) router.history.push(tvHref);
  }, [declined, router]);

  const err = query.error;
  let failure: { title: string; description: string } | null = null;
  if (query.isError && !declined) {
    if (err instanceof OffAirError)
      failure = {
        title: tr('cinema.liveWatch.offAirTitle'),
        description: tr('cinema.liveWatch.offAirBody'),
      };
    else if (isNotAvailable(err) && err instanceof ApiError && err.status !== 404)
      failure = {
        title: tr('cinema.liveWatch.unavailableTitle'),
        description: tr('cinema.liveWatch.unavailableBody'),
      };
    else if (err instanceof ApiError && err.status === 404)
      failure = {
        title: tr('cinema.liveWatch.goneTitle'),
        description: tr('cinema.liveWatch.goneBody'),
      };
    else
      failure = {
        title: tr('cinema.liveWatch.errorTitle'),
        description: tr('cinema.liveWatch.errorBody'),
      };
  }
  const tuning = !reuse && (query.isFetching || (!!grant && status === 'loading'));

  return (
    <LiveView
      channelId={channelId}
      grant={grant}
      tuning={tuning}
      failure={failure}
      onRetune={retune}
      onSwitch={(id) => router.history.replace(watchChannelHref(id))}
      onExit={exit}
    />
  );
}
