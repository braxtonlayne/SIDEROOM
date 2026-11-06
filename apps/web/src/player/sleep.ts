/**
 * MUS-SKN-002 (D-522, D-525): the sleep timer's watcher. The store holds only `sleepAt` (epoch ms, or null); this
 * owns the one timeout and the fade. Thirty seconds before `sleepAt` the master gain ramps to silence on the
 * audio clock; at `sleepAt` the player pauses and the gain goes back to the member's volume. Clearing `sleepAt`
 * (the skin's button, `stop`, `clear`) cancels a pending timeout and, mid-fade, restores the gain at once.
 *
 * Without Web Audio there is no gain to ramp: the player simply pauses at `sleepAt`. Reduced motion has no
 * bearing here (this is sound, not motion). `PlayerHost` starts the watcher once, next to the engine.
 */
import { applyVolume, type AudioGraph } from './graph';
import { pause, usePlayerStore } from './store';

/** How long the fade before the pause lasts. */
export const SLEEP_FADE_MS = 30_000;

export interface SleepTimers {
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
  now(): number;
}

const realTimers: SleepTimers = {
  setTimeout: (fn, ms) => globalThis.setTimeout(fn, ms),
  clearTimeout: (h) => globalThis.clearTimeout(h as ReturnType<typeof setTimeout>),
  now: () => Date.now(),
};

/** Watch `sleepAt`; returns the stopper. `graph` is read when the fade starts (the engine builds it lazily). */
export function startSleepWatcher(
  graph: () => AudioGraph | null,
  timers: SleepTimers = realTimers,
): () => void {
  const store = usePlayerStore;
  let handle: unknown = null;
  /** The graph whose master gain is ramping now, if any. */
  let fading: AudioGraph | null = null;

  const restore = () => {
    const g = fading;
    fading = null;
    if (!g) return;
    const gain = g.master.gain;
    gain.cancelScheduledValues(g.ctx.currentTime);
    const s = store.getState();
    applyVolume(g, s.volume, s.muted);
  };

  const cancel = () => {
    if (handle !== null) timers.clearTimeout(handle);
    handle = null;
    restore();
  };

  const finish = () => {
    handle = null;
    // Pause first (the engine pauses the element on this store change), then put the gain back.
    pause();
    store.setState({ sleepAt: null });
    restore();
  };

  const fade = (at: number) => {
    const left = Math.max(0, at - timers.now());
    const g = graph();
    if (g && left > 0) {
      const gain = g.master.gain;
      const t = g.ctx.currentTime;
      gain.cancelScheduledValues(t);
      gain.setValueAtTime(gain.value, t);
      gain.linearRampToValueAtTime(0, t + left / 1000);
      fading = g;
    }
    handle = timers.setTimeout(finish, left);
  };

  const arm = (at: number | null) => {
    cancel();
    if (at === null) return;
    const wait = at - SLEEP_FADE_MS - timers.now();
    if (wait <= 0) fade(at);
    else handle = timers.setTimeout(() => fade(at), wait);
  };

  arm(store.getState().sleepAt);
  const unsubscribe = store.subscribe((s, prev) => {
    if (s.sleepAt !== prev.sleepAt) arm(s.sleepAt);
  });
  return () => {
    unsubscribe();
    cancel();
  };
}
