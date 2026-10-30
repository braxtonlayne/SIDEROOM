/**
 * UTL-DEV-001: `useFocusTimer()`, the reusable face of the timers. A tool, a gadget, the pin rail or a music
 * player skin calls this and gets the same shared timer, already ticking.
 *
 *   const t = useFocusTimer('pomodoro');
 *   <span>{t.clock}</span> <button onClick={t.toggle}>{t.running ? 'Pause' : 'Start'}</button>
 *
 * It renders at most four times a second while running and not at all while at rest or paused.
 */
import { useEffect, useState, useSyncExternalStore } from 'react';
import {
  type PomodoroConfig,
  type Segment,
  type TimerKind,
  type TimerState,
  elapsedMs,
  formatClock,
  remainingMs,
} from './timerCore';
import { type TimerStore, timerStore } from './timerStore';

export interface FocusTimer {
  kind: TimerKind;
  state: TimerState;
  config: PomodoroConfig;
  running: boolean;
  /** Time left (pomodoro, countdown); `null` for a stopwatch. */
  remainingMs: number | null;
  elapsedMs: number;
  /** The face of the clock: time left, or elapsed for a stopwatch. */
  clock: string;
  /** 0 to 1 through the current segment; 0 for a stopwatch. */
  progress: number;
  start: () => void;
  pause: () => void;
  toggle: () => void;
  reset: () => void;
  skip: () => void;
  setCountdownMs: (ms: number) => void;
  setConfig: (patch: Partial<PomodoroConfig>) => void;
}

export interface FocusTimerOptions {
  /** Called when a segment ends (a focus session, a break or a countdown). */
  onFinish?: (finished: Segment[], state: TimerState) => void;
  /** Use a store other than the shared one (tests, a second independent timer). */
  store?: TimerStore;
  /** Tick interval while running, in ms. */
  tickMs?: number;
}

export function useFocusTimer(kind: TimerKind = 'pomodoro', opts: FocusTimerOptions = {}): FocusTimer {
  const store = opts.store ?? timerStore(kind);
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const [now, setNow] = useState(() => Date.now());
  const { onFinish, tickMs = 250 } = opts;

  useEffect(() => {
    if (!onFinish) return undefined;
    return store.onFinish(onFinish);
  }, [store, onFinish]);

  useEffect(() => {
    setNow(Date.now());
    if (state.phase !== 'running') return undefined;
    const id = setInterval(() => {
      store.settle();
      setNow(Date.now());
    }, tickMs);
    return () => clearInterval(id);
  }, [store, state.phase, tickMs]);

  const t = state.phase === 'running' ? now : (state.startedAt ?? now);
  const left = remainingMs(state, t);
  const done = elapsedMs(state, t);
  return {
    kind,
    state,
    config: store.getConfig(),
    running: state.phase === 'running',
    remainingMs: left,
    elapsedMs: done,
    clock: formatClock(left ?? done, left === null),
    progress: state.durationMs ? Math.min(1, done / state.durationMs) : 0,
    start: store.start,
    pause: store.pause,
    toggle: store.toggle,
    reset: store.reset,
    skip: store.skip,
    setCountdownMs: store.setCountdownMs,
    setConfig: store.setConfig,
  };
}
