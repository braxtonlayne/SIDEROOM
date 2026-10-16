import { useSyncExternalStore } from 'react';

/** Relative timestamps re-render once a minute (Q91). */
export const RELATIVE_TICK_MS = 60_000;

const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | undefined;

function emit(): void {
  for (const l of listeners) l();
}

/** One shared interval for every relative timestamp on the page; stopped when none is mounted. */
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  timer ??= setInterval(emit, RELATIVE_TICK_MS);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

/** The current minute. It changes at most once per tick, so subscribers re-render then. */
function minuteSnapshot(): number {
  return Math.floor(Date.now() / RELATIVE_TICK_MS);
}

function noSubscribe(): () => void {
  return () => undefined;
}

/**
 * The reference "now" for a relative timestamp. Re-renders on each shared tick when enabled.
 * `fixed` (the `now` option) wins and never ticks.
 */
export function useRelativeNow(enabled: boolean, fixed?: number): number | undefined {
  useSyncExternalStore(
    enabled && fixed === undefined ? subscribe : noSubscribe,
    minuteSnapshot,
    minuteSnapshot,
  );
  if (fixed !== undefined) return fixed;
  return enabled ? Date.now() : undefined;
}
