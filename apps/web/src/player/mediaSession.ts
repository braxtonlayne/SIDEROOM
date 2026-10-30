/**
 * MUS-PLY-001: the Media Session API, so the lock screen, a headset button and the browser's media hub show
 * the track and drive the same actions as the skins. Everything is feature-detected.
 */
import { next, pause, play, prev, seek } from './store';
import type { PlayerStatus, PlayerTrack } from './types';

type MS = MediaSession;

const session = (): MS | null =>
  typeof navigator !== 'undefined' && 'mediaSession' in navigator ? navigator.mediaSession : null;

export function updateMetadata(track: PlayerTrack | undefined): void {
  const ms = session();
  if (!ms) return;
  if (!track || typeof MediaMetadata === 'undefined') {
    ms.metadata = null;
    return;
  }
  ms.metadata = new MediaMetadata({
    title: track.title,
    artist: track.artist ?? '',
    album: track.album ?? '',
    artwork: track.artworkUrl ? [{ src: track.artworkUrl }] : [],
  });
}

export function updatePlaybackState(status: PlayerStatus): void {
  const ms = session();
  if (!ms) return;
  ms.playbackState = status === 'playing' ? 'playing' : status === 'idle' ? 'none' : 'paused';
}

export function updatePosition(position: number, duration: number): void {
  const ms = session();
  if (!ms || !ms.setPositionState || !(duration > 0) || !Number.isFinite(duration)) return;
  try {
    ms.setPositionState({
      duration,
      position: Math.min(Math.max(0, position), duration),
      playbackRate: 1,
    });
  } catch {
    /* an out-of-range state is not worth a crash */
  }
}

/** Install the action handlers; returns the remover. */
export function installActions(): () => void {
  const ms = session();
  if (!ms) return () => undefined;
  const handlers: [MediaSessionAction, MediaSessionActionHandler][] = [
    ['play', () => play()],
    ['pause', () => pause()],
    ['previoustrack', () => prev()],
    ['nexttrack', () => next()],
    ['seekto', (d) => typeof d.seekTime === 'number' && seek(d.seekTime)],
  ];
  for (const [action, h] of handlers) {
    try {
      ms.setActionHandler(action, h);
    } catch {
      /* an action the browser does not know */
    }
  }
  return () => {
    for (const [action] of handlers) {
      try {
        ms.setActionHandler(action, null);
      } catch {
        /* same */
      }
    }
  };
}
