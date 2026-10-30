/**
 * MUS-PLY-001 (wave-30 integration, D-385 "only the code that is absolutely necessary" on public pages): the
 * part of the player the entrance bundle carries. `PlayerGate` sits in the root layout, so whatever it imports
 * loads on the landing page and the login form too. It therefore never imports the player store: it reads this
 * tiny flag, which the store keeps up to date once something (a signed-in page) has loaded it.
 *
 * `forgetPlayer` is what sign-out and a move onto a public page call. It reaches the store only when the store
 * is already loaded (it registers itself here); otherwise there is nothing to forget and nothing is fetched.
 *
 * MUS-PLY-002: the rail's music button also lives on this flag (see `openPlayerLibrary`).
 */
import { useSyncExternalStore } from 'react';

let queued = false;
let libraryOpen = false;
let libraryNonce = 0;
let forget: (() => void) | null = null;
const listeners = new Set<() => void>();

function notify(): void {
  for (const l of listeners) l();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** The store reports whether anything is queued. */
export function setPlayerQueued(on: boolean): void {
  if (on === queued) return;
  queued = on;
  notify();
}

/** The store registers its stop-and-forget action when it loads. */
export function registerPlayerForget(fn: () => void): void {
  forget = fn;
}

/** Stop playback and drop the queue, if the player was ever loaded on this page. Also closes the library view. */
export function forgetPlayer(): void {
  closePlayerLibrary();
  forget?.();
}

/**
 * MUS-PLY-002: the rail's music button. Opens the floating player on its Library tab even when nothing is queued.
 * Like the queued flag it lives here, not in the store, so the rail and the gate can ask for the player without
 * loading it; the host chunk reads the flag when it mounts. Every call bumps `libraryNonce`, so an already open
 * player is switched to the library skin and its Library tab again.
 */
export function openPlayerLibrary(): void {
  libraryOpen = true;
  libraryNonce += 1;
  notify();
}

/** The player's close button, and a move onto a public page, end the library-only view. */
export function closePlayerLibrary(): void {
  if (!libraryOpen) return;
  libraryOpen = false;
  notify();
}

export const isPlayerQueued = (): boolean => queued;
export const isPlayerLibraryOpen = (): boolean => libraryOpen;

export function usePlayerQueued(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => queued,
    () => false,
  );
}

/** Whether the library view was asked for (the rail button) and has not been closed since. */
export function usePlayerLibraryOpen(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => libraryOpen,
    () => false,
  );
}

/** Changes on every `openPlayerLibrary` call. */
export function usePlayerLibraryNonce(): number {
  return useSyncExternalStore(
    subscribe,
    () => libraryNonce,
    () => 0,
  );
}
