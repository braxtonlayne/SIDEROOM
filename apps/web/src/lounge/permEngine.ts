/**
 * Channel-permission seam for the Lounge (D-178).
 *
 * The computation is the Rust `sr-permissions-wasm` `compute` (roles, channel overwrites, timeout rule),
 * loaded lazily through `@sideroom/wasm`. `loadPermEngine()` starts the load (the shell calls it on mount).
 * Until it resolves the Lounge falls back to the guild-level bits, so nothing waits on the download. The
 * server stays the security boundary: this only decides which controls to show.
 */
import type { ComputeChannelInput } from '@sideroom/wasm';
import { useSyncExternalStore } from 'react';

export interface PermEngine {
  /** Decimal-string channel permissions for one member (overwrites, timeout and implicit rules applied). */
  compute(input: ComputeChannelInput): string;
}

let engine: PermEngine | null = null;
let loading: Promise<PermEngine> | null = null;
const listeners = new Set<() => void>();

function publish(next: PermEngine | null) {
  engine = next;
  for (const listener of listeners) listener();
}

/** Swaps the active engine; `null` returns to the guild-level fallback. Tests inject stand-ins. */
export function setPermEngine(next: PermEngine | null): void {
  publish(next);
}

export function currentPermEngine(): PermEngine | null {
  return engine;
}

/** Loads the WASM engine once. Resolves at once when an engine is already active. */
export function loadPermEngine(): Promise<PermEngine> {
  if (engine) return Promise.resolve(engine);
  loading ??= import('@sideroom/wasm')
    .then(({ loadPermissions }) => loadPermissions())
    .then(
      (api) => {
        const loaded: PermEngine = { compute: (input) => api.compute(input) };
        publish(loaded);
        return loaded;
      },
      (err: unknown) => {
        loading = null;
        throw err;
      },
    );
  return loading;
}

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
const getEngine = () => engine;

/** The engine once loaded (re-renders the caller on load), otherwise `null`. */
export function usePermEngine(): PermEngine | null {
  return useSyncExternalStore(subscribe, getEngine, getEngine);
}
