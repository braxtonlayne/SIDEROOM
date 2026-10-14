// The real wiring of the sync controller: the Cinema player session as the `SyncPlayer`, the HTTP API, the
// gateway dispatch seam and the signed-in account. One controller per app, like the player session.
import { useStore as useZustand } from 'zustand';
import { onDispatch } from '../store/events';
import { holdGateway } from '../store/connect';
import { useStore } from '../store/store';
import { getPlayback } from '../cinema/player/api';
import { setMiniExpandHref } from '../cinema/player/bridge';
import { playerSession, type PlayerSession } from '../cinema/player/session';
import { httpPartyApi } from './api';
import { PartyController, type PartyState, type SyncPlayer } from './controller';
import type { PartyMedia } from './types';

/** The Cinema player session as a `SyncPlayer`. Position is in ms; the element is the session's own. */
export function sessionPlayer(session: PlayerSession): SyncPlayer {
  return {
    positionMs: () => session.element().currentTime * 1000,
    paused: () => session.element().paused,
    waiting: () => session.state.waiting,
    async play() {
      await session.play();
      return !session.state.needsGesture;
    },
    pause: () => session.pause(),
    seekMs: (ms) => session.seek(ms / 1000, false),
    // Straight on the element: `session.setRate` announces "1.05x" in the player feedback overlay.
    setRate: (r) => {
      session.element().playbackRate = r;
    },
    subscribe(fn) {
      return session.store.subscribe((s, prev) => {
        if (s.waiting && !prev.waiting) fn('waiting');
        else if (!s.waiting && prev.waiting) fn('playing');
        if (s.ended && !prev.ended) fn('ended');
      });
    },
  };
}

/** Open the party's media in the session (idempotent when it is already the open item). */
export async function loadPartyMedia(
  session: PlayerSession,
  media: PartyMedia,
  startMs: number,
  autoplay: boolean,
): Promise<void> {
  const episode = media.episode_id !== '0' ? media.episode_id : null;
  if (session.matches(media.title_id, episode) && session.state.status !== 'error') {
    session.setMode('full');
    return;
  }
  const info = await getPlayback({
    titleId: media.title_id,
    episodeId: episode,
    variantId: media.variant_id,
  });
  await session.start(info, { startAt: startMs / 1000, autoplay });
  session.setMode('full');
}

let controller: PartyController | null = null;

export function partyController(): PartyController {
  if (!controller) {
    const session = playerSession();
    controller = new PartyController({
      api: httpPartyApi,
      player: sessionPlayer(session),
      load: (m, start, autoplay) => loadPartyMedia(playerSession(), m, start, autoplay),
      now: () => Date.now(),
      me: () => useStore.getState().currentUserId ?? '',
      onDispatch: (fn) => onDispatch(fn),
      hold: holdGateway,
    });
    // Closing the mini player (the session goes idle) while following a party means leaving it.
    session.store.subscribe((s, prev) => {
      if (prev.status !== 'idle' && s.status === 'idle' && controller?.state.status === 'active')
        void controller.close({ leave: true });
    });
    // The mini player expands back into the party instead of the plain player while a party is followed.
    controller.store.subscribe((s) => {
      setMiniExpandHref(s.status === 'active' && s.party ? `/cinema/party/${s.party.id}` : null);
    });
  }
  return controller;
}

/** Tests: install a controller with fake dependencies, or `null` to drop it. */
export function setPartyController(c: PartyController | null) {
  if (controller && controller !== c) void controller.close();
  controller = c;
}

export function useParty<T>(selector: (s: PartyState) => T): T {
  return useZustand(partyController().store, selector);
}
