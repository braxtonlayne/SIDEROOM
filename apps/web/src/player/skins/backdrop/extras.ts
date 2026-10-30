/**
 * MUS-SKN-001: the small working extras a skin can show (a focus timer and a notes pad), behind one adapter.
 *
 * The real tools are wave 32's shared ones (UTL-DEV-001): `useFocusTimer()` in `tools/timers/useFocusTimer.ts`
 * and `useNotes()` in `tools/notes/useNotes.ts`, the same timer and the same encrypted scratchpad the utility
 * drawer shows. They are not on this branch yet, so the skins read them through `SkinExtras` and the integrator
 * binds them with `registerSkinExtras` once wave 32 lands (see the handoff, feat-p34-player-skins.md):
 *
 *   registerSkinExtras({
 *     useFocusTimer: () => { const f = useFocusTimer('pomodoro');
 *       return { clock: f.clock, running: f.running, progress: f.progress, segment: f.state.segment,
 *                toggle: f.toggle, reset: f.reset, skip: f.skip }; },
 *     useNotes: () => { ...map useNotes() to SkinNotes... },
 *   });
 *
 * Until then the defaults below run: a minimal working pomodoro (25 minutes of focus, 5 of break, started and
 * paused by hand, kept in memory for this tab) and a notes pad that lives in memory for this tab only and says
 * so. Nothing is written to storage: the member's notes belong in wave 32's end-to-end encrypted scratchpad, not
 * in plain localStorage.
 */
import { useEffect, useState, useSyncExternalStore } from 'react';

export type FocusSegment = 'focus' | 'short' | 'long';

/** The subset of wave 32's `FocusTimer` a skin shows. */
export interface SkinFocusTimer {
  /** The face of the clock, `mm:ss`. */
  clock: string;
  running: boolean;
  /** 0..1 through the current segment. */
  progress: number;
  segment: FocusSegment;
  toggle: () => void;
  reset: () => void;
  /** Go to the next segment (focus to break, break to focus). */
  skip: () => void;
}

export interface SkinNotes {
  status: 'ready' | 'loading' | 'unavailable';
  text: string;
  setText: (text: string) => void;
  /** Saved beyond this tab (wave 32's encrypted scratchpad) or only in memory (the default). */
  persistent: boolean;
}

export interface SkinExtras {
  useFocusTimer: () => SkinFocusTimer;
  useNotes: () => SkinNotes;
}

// ---- The default focus timer: a minimal pomodoro in memory ----

export const FOCUS_MS = 25 * 60_000;
export const BREAK_MS = 5 * 60_000;

interface PomoState {
  segment: FocusSegment;
  /** Time counted in the segment before the current run. */
  accumulated: number;
  /** Epoch ms the current run began, or null when paused. */
  startedAt: number | null;
}

const lengthOf = (seg: FocusSegment) => (seg === 'focus' ? FOCUS_MS : BREAK_MS);

/** A tiny store so the timer survives a skin change (the skin unmounts) while the tab stays open. */
export function createPomodoro(now: () => number = Date.now) {
  let state: PomoState = { segment: 'focus', accumulated: 0, startedAt: null };
  const subs = new Set<() => void>();
  const emit = () => subs.forEach((f) => f());
  const elapsed = (s: PomoState, at: number) =>
    s.accumulated + (s.startedAt === null ? 0 : at - s.startedAt);
  const api = {
    getState: () => state,
    subscribe(fn: () => void) {
      subs.add(fn);
      return () => subs.delete(fn);
    },
    elapsed: (at = now()) => elapsed(state, at),
    /** End a segment that ran out: the next one starts paused, the way the member left it. */
    settle(at = now()) {
      if (state.startedAt !== null && elapsed(state, at) >= lengthOf(state.segment)) {
        state = {
          segment: state.segment === 'focus' ? 'short' : 'focus',
          accumulated: 0,
          startedAt: null,
        };
        emit();
      }
    },
    toggle() {
      const at = now();
      state =
        state.startedAt === null
          ? { ...state, startedAt: at }
          : { ...state, accumulated: elapsed(state, at), startedAt: null };
      emit();
    },
    reset() {
      state = { segment: 'focus', accumulated: 0, startedAt: null };
      emit();
    },
    skip() {
      state = {
        segment: state.segment === 'focus' ? 'short' : 'focus',
        accumulated: 0,
        startedAt: state.startedAt === null ? null : now(),
      };
      emit();
    },
  };
  return api;
}

export type Pomodoro = ReturnType<typeof createPomodoro>;

const clockOf = (ms: number) => {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
};

export function usePomodoro(p: Pomodoro): SkinFocusTimer {
  const state = useSyncExternalStore(p.subscribe, p.getState, p.getState);
  const [, tick] = useState(0);
  const running = state.startedAt !== null;
  useEffect(() => {
    if (!running) return undefined;
    const id = setInterval(() => {
      p.settle();
      tick((n) => n + 1);
    }, 250);
    return () => clearInterval(id);
  }, [p, running]);
  const total = lengthOf(state.segment);
  const done = Math.min(total, p.elapsed());
  return {
    clock: clockOf(total - done),
    running,
    progress: done / total,
    segment: state.segment,
    toggle: p.toggle,
    reset: p.reset,
    skip: p.skip,
  };
}

const defaultPomodoro = createPomodoro();

// ---- The default notes pad: memory only, for this tab ----

let memoText = '';
const memoSubs = new Set<() => void>();
const memo = {
  subscribe(fn: () => void) {
    memoSubs.add(fn);
    return () => memoSubs.delete(fn);
  },
  get: () => memoText,
  set(text: string) {
    memoText = text;
    memoSubs.forEach((f) => f());
  },
};

function useMemoryNotes(): SkinNotes {
  const text = useSyncExternalStore(memo.subscribe, memo.get, memo.get);
  return { status: 'ready', text, setText: memo.set, persistent: false };
}

export const DEFAULT_SKIN_EXTRAS: SkinExtras = {
  useFocusTimer: () => usePomodoro(defaultPomodoro),
  useNotes: useMemoryNotes,
};

// ---- Registration ----

let current: SkinExtras = DEFAULT_SKIN_EXTRAS;
let version = 0;
const subs = new Set<() => void>();

/**
 * Bind the real tools. Missing hooks keep the default. Returns a function that puts the defaults back. Call it
 * once at start-up (the hooks must not change while a skin is mounted; a change remounts the extras).
 */
export function registerSkinExtras(extras: Partial<SkinExtras>): () => void {
  const next = { ...DEFAULT_SKIN_EXTRAS, ...extras };
  current = next;
  version++;
  subs.forEach((f) => f());
  return () => {
    if (current !== next) return;
    current = DEFAULT_SKIN_EXTRAS;
    version++;
    subs.forEach((f) => f());
  };
}

/** The bound extras and a version to key the component that calls their hooks. */
export function useSkinExtras(): { extras: SkinExtras; version: number } {
  const v = useSyncExternalStore(
    (fn) => {
      subs.add(fn);
      return () => subs.delete(fn);
    },
    () => version,
    () => version,
  );
  return { extras: current, version: v };
}

/** Test hook. */
export function __resetSkinExtras(): void {
  current = DEFAULT_SKIN_EXTRAS;
  version++;
  defaultPomodoro.reset();
  memo.set('');
}
