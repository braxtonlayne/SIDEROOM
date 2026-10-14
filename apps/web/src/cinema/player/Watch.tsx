// The watch route page (CIN-PLY-001, 008, 011, D-184): loads the playback info (through the shared age gate),
// starts the session, offers to resume, and renders the player. Coming back from the mini player re-attaches the
// running session instead of loading anything again.
import { EmptyState, Spinner } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { useRouter } from '@tanstack/react-router';
import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { isAgeGateError, useAgeGate, withAgeGate } from '../../lib/ageGate';
import { getPlayback } from './api';
import { resumeOffer } from './model';
import { PlayerView } from './Player';
import { progressApi } from './progress';
import { playerSession } from './session';

export interface WatchPageProps {
  titleId: string;
  episodeId?: string;
  variantId?: string;
  trailer?: boolean;
  /** Start position in seconds from the link (`?t=`). */
  startAt?: number;
}

export function WatchPage(props: WatchPageProps) {
  // A different title or episode is a different playback: all state starts over.
  return (
    <WatchLoader
      key={`${props.titleId}:${props.episodeId ?? ''}:${props.variantId ?? ''}:${props.trailer ? 't' : ''}`}
      {...props}
    />
  );
}

/** Browsers allow sound without a click only after the page has had user activation. */
const hasActivation = () =>
  typeof navigator !== 'undefined' &&
  (navigator as Navigator & { userActivation?: { hasBeenActive: boolean } }).userActivation
    ?.hasBeenActive === true;

function WatchLoader({ titleId, episodeId, variantId, trailer, startAt }: WatchPageProps) {
  const router = useRouter();
  const { ensureAdult } = useAgeGate();
  const s = playerSession();
  const [reuse] = useState(() => s.matches(titleId, episodeId ?? null) && !trailer);
  const [resumeAt, setResumeAt] = useState<number | null>(null);

  const query = useQuery({
    queryKey: ['cinema', 'playback', titleId, episodeId ?? null, variantId ?? null, !!trailer],
    // 403 900011 -> "Are you 18 or older?" -> on yes the request runs once more (D-184).
    queryFn: () =>
      withAgeGate(() => getPlayback({ titleId, episodeId, variantId, trailer }), ensureAdult),
    enabled: !reuse,
    retry: false,
    // Signed URLs: never serve a cached copy.
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  const info = reuse ? s.state.info : query.data;

  const back = useCallback(() => {
    if (router.history.canGoBack()) router.history.back();
    else router.history.push(`/cinema/t/${encodeURIComponent(titleId)}`);
  }, [router, titleId]);

  const declined = isAgeGateError(query.error);
  useEffect(() => {
    // The player stays closed (D-184): back to where the viewer came from.
    if (declined) back();
  }, [declined, back]);

  const mediaId = info?.media_id;
  useEffect(() => {
    if (reuse || !info) return;
    let dead = false;
    void (async () => {
      const saved =
        info.progress ??
        (info.trailer ? null : await progressApi.get(info.title_id, info.episode_id));
      if (dead) return;
      const offer = resumeOffer(saved, startAt);
      setResumeAt(offer);
      await s.start(info, {
        startAt: startAt ?? undefined,
        autoplay: offer === null && hasActivation(),
      });
    })();
    return () => {
      dead = true;
    };
    // `info` is stable for a given media id; starting again on every refetch would restart playback.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mediaId, reuse]);

  if (query.isError && !declined) {
    const gone = query.error instanceof ApiError && query.error.status === 404;
    return (
      <div className="cpl-player cpl-player-msg">
        <EmptyState
          title={gone ? 'This title is not available' : 'We could not open this title'}
          description={
            gone
              ? 'It may have been unpublished or removed.'
              : 'Check your connection and try again.'
          }
          action={
            <a className="cpl-primary" href="/cinema">
              Back to the cinema
            </a>
          }
        />
      </div>
    );
  }
  if (!info || declined) {
    return (
      <div className="cpl-player cpl-player-msg">
        <Spinner label="Opening title" />
      </div>
    );
  }
  return (
    <PlayerView
      resumeAt={resumeAt}
      onResumeHandled={() => setResumeAt(null)}
      navigate={(href) => router.history.push(href)}
      onExit={back}
    />
  );
}
