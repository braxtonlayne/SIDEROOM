/**
 * MUS-PLY-001 (wave-30 integration, D-385 "only the code that is absolutely necessary" on public pages): the
 * part of the player the entrance bundle carries. `PlayerGate` sits in the root layout, so whatever it imports
 * loads on the landing page and the login form too. It therefore never imports the player store: it reads this
 * tiny flag, which the store keeps up to date once something (a signed-in page) has loaded it.
 *
 * `forgetPlayer` is what sign-out and a move onto a public page call. It reaches the store only when the store
 * is already loaded (it registers itself here); otherwise there is nothing to forget and nothing is fetched.
 */
import { useSyncExternalStore } from 'react';

let queued = false;
let forget: (() => void) | null = null;
const listeners = new Set<() => void>();

/** The store reports whether anything is queued. */
export function setPlayerQueued(on: boolean): void {
  if (on === queued) return;
  queued = on;
  for (const l of listeners) l();
}

/** The store registers its stop-and-forget action when it loads. */
export function registerPlayerForget(fn: () => void): void {
  forget = fn;
}

/** Stop playback and drop the queue, if the player was ever loaded on this page. */
export function forgetPlayer(): void {
  forget?.();
}

export const isPlayerQueued = (): boolean => queued;

export function usePlayerQueued(): boolean {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => {
        listeners.delete(cb);
      };
    },
    () => queued,
    () => false,
  );
}
