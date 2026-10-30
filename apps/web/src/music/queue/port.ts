/**
 * MUS-LIB-001: the one door from the library panel to the player.
 *
 * The player store belongs to player-shell (`player/store.ts`); music-lib drives it through its exported
 * actions only. This module is that contract as an interface in music-lib's own directory: the panel calls
 * `getPlayerPort()`, and the app registers an implementation once at start-up (`setPlayerPort`) that forwards
 * to `play`, `enqueue`, `setEq`, `setEqEnabled`, `setCrossfade` and `usePlayerStore`. The handoff has the
 * ten-line adapter. Until one is registered the port is inert (nothing queued, actions do nothing), so the
 * panel renders and is testable without a player.
 */
import { useSyncExternalStore } from 'react';
import type { PortTrack } from './types';

/** What the panel reads from the player. */
export interface PortState {
  /** Ids in the queue, in order. */
  queueIds: readonly string[];
  /** Index of the playing track, -1 for none. */
  index: number;
  /** Gain in dB per EQ band. */
  eq: readonly number[];
  eqEnabled: boolean;
  /** Seconds of crossfade, 0 to 12. */
  crossfadeSec: number;
}

export interface PlayerPort {
  /** Replace the queue and start at `startAt`. */
  play(tracks: readonly PortTrack[], startAt?: number): void;
  /** Add to the end of the queue, or right after the current track. */
  enqueue(tracks: readonly PortTrack[], opts?: { next?: boolean }): void;
  setEq(bands: readonly number[]): void;
  setEqEnabled(on: boolean): void;
  setCrossfade(seconds: number): void;
  /** A snapshot; must return the same object while nothing changed (it feeds `useSyncExternalStore`). */
  getState(): PortState;
  subscribe(listener: () => void): () => void;
}

const IDLE: PortState = {
  queueIds: [],
  index: -1,
  eq: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  eqEnabled: true,
  crossfadeSec: 0,
};

const inertPort: PlayerPort = {
  play: () => undefined,
  enqueue: () => undefined,
  setEq: () => undefined,
  setEqEnabled: () => undefined,
  setCrossfade: () => undefined,
  getState: () => IDLE,
  subscribe: () => () => undefined,
};

let port: PlayerPort = inertPort;
const watchers = new Set<() => void>();
let detach: () => void = () => undefined;

function notify(): void {
  watchers.forEach((w) => w());
}

function attach(p: PlayerPort): void {
  detach();
  port = p;
  detach = p.subscribe(notify);
  notify();
}

/** Register the player. Returns a function that puts the inert port back. */
export function setPlayerPort(p: PlayerPort): () => void {
  attach(p);
  return () => {
    if (port === p) attach(inertPort);
  };
}

export const getPlayerPort = (): PlayerPort => port;

/** The player's state, live. */
export function usePortState(): PortState {
  return useSyncExternalStore(
    (cb) => {
      watchers.add(cb);
      return () => {
        watchers.delete(cb);
      };
    },
    () => port.getState(),
    () => IDLE,
  );
}
