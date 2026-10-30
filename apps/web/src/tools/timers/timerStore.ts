/**
 * UTL-DEV-001: one shared timer per kind. The drawer, a popped-out gadget, the side rail and (later) a music
 * player skin all read the same store, so a pomodoro started in one place is the same pomodoro everywhere.
 * The state survives a reload through `localStorage` (a convenience: it can be absent or blocked, and the
 * timer then simply starts at rest).
 */
import {
  type Advance,
  DEFAULT_POMODORO,
  type PomodoroConfig,
  type Segment,
  type TimerKind,
  type TimerState,
  advance,
  initialState,
  pause as pauseState,
  reset as resetState,
  setCountdown,
  skip as skipState,
  start as startState,
} from './timerCore';

export interface TimerStore {
  readonly kind: TimerKind;
  getState(): TimerState;
  getConfig(): PomodoroConfig;
  subscribe(listener: () => void): () => void;
  /** Called with the segments that just ended (a focus session, a break, a countdown). */
  onFinish(listener: (finished: Segment[], state: TimerState) => void): () => void;
  start(): void;
  pause(): void;
  toggle(): void;
  reset(): void;
  skip(): void;
  setCountdownMs(ms: number): void;
  setConfig(patch: Partial<PomodoroConfig>): void;
  /** Settle against the clock: ends segments that ran out. The hook calls it on every tick. */
  settle(): void;
  /** Replace the state (a timer received from a voice channel, a test). */
  replace(next: TimerState): void;
}

export interface StoreDeps {
  now?: () => number;
  storage?: Pick<Storage, 'getItem' | 'setItem'> | null;
}

const KEY = 'sr.tools.timer.v1.';

function safeStorage(): Pick<Storage, 'getItem' | 'setItem'> | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

interface Saved {
  state: TimerState;
  config: PomodoroConfig;
}

function load(kind: TimerKind, storage: StoreDeps['storage']): Saved | null {
  try {
    const raw = storage?.getItem(KEY + kind);
    if (!raw) return null;
    const v = JSON.parse(raw) as Partial<Saved>;
    if (!v.state || v.state.kind !== kind) return null;
    const cfg = { ...DEFAULT_POMODORO, ...(v.config ?? {}) };
    return { state: v.state, config: cfg };
  } catch {
    return null;
  }
}

export function createTimerStore(kind: TimerKind, deps: StoreDeps = {}): TimerStore {
  const now = deps.now ?? Date.now;
  const storage = deps.storage === undefined ? safeStorage() : deps.storage;
  const saved = load(kind, storage);
  let config: PomodoroConfig = saved?.config ?? { ...DEFAULT_POMODORO };
  let state: TimerState = saved?.state ?? initialState(kind, config);
  const listeners = new Set<() => void>();
  const finishers = new Set<(f: Segment[], s: TimerState) => void>();

  const persist = () => {
    try {
      storage?.setItem(KEY + kind, JSON.stringify({ state, config } satisfies Saved));
    } catch {
      /* private window or full storage: the timer still works */
    }
  };
  const set = (next: TimerState) => {
    if (next === state) return;
    state = next;
    persist();
    for (const l of [...listeners]) l();
  };
  const apply = (r: Advance) => {
    set(r.state);
    if (r.finished.length) for (const f of [...finishers]) f(r.finished, r.state);
  };

  const store: TimerStore = {
    kind,
    getState: () => state,
    getConfig: () => config,
    subscribe(l) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    onFinish(l) {
      finishers.add(l);
      return () => finishers.delete(l);
    },
    start() {
      apply(advance(startState(state, now()), now(), config));
    },
    pause() {
      set(pauseState(state, now()));
    },
    toggle() {
      if (state.phase === 'running') store.pause();
      else store.start();
    },
    reset() {
      set(resetState(state, config));
    },
    skip() {
      apply(advance(skipState(state, now(), config), now(), config));
    },
    setCountdownMs(ms) {
      set(setCountdown(state, ms));
    },
    setConfig(patch) {
      config = { ...config, ...patch };
      // A pomodoro at rest picks up a new length at once; a running one keeps its segment.
      if (kind === 'pomodoro' && state.phase === 'idle') state = initialState(kind, config);
      persist();
      for (const l of [...listeners]) l();
    },
    settle() {
      if (state.phase !== 'running') return;
      const r = advance(state, now(), config);
      if (r.state !== state) apply(r);
    },
    replace(next) {
      set(next);
    },
  };
  // Catch up on whatever happened while the page was closed.
  store.settle();
  return store;
}

const shared = new Map<TimerKind, TimerStore>();

/** The page-wide timer of a kind. */
export function timerStore(kind: TimerKind): TimerStore {
  let s = shared.get(kind);
  if (!s) {
    s = createTimerStore(kind);
    shared.set(kind, s);
  }
  return s;
}

/** Tests: forget the shared stores. */
export function resetSharedTimers(): void {
  shared.clear();
}
