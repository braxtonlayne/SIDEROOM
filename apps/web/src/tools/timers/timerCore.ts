/**
 * UTL-DEV-001: the timers, as pure functions of a state and a clock reading. No React, no DOM, no storage: the
 * hook (`useFocusTimer`), the tools and, later, a player skin all drive the same state machine.
 *
 * A timer runs by remembering when the current run segment started (`startedAt`, epoch ms) and how much time
 * earlier segments added (`accumulatedMs`). Nothing counts ticks, so a throttled background tab, a laptop
 * lid or a reload cannot make it drift: the elapsed time is always `accumulated + (now - startedAt)`.
 */

export type TimerKind = 'pomodoro' | 'stopwatch' | 'countdown';
export type TimerPhase = 'idle' | 'running' | 'paused' | 'done';
/** Which stretch of a pomodoro cycle is on (always `focus` for the other kinds). */
export type Segment = 'focus' | 'short' | 'long';

export interface PomodoroConfig {
  focusMs: number;
  shortMs: number;
  longMs: number;
  /** Focus sessions before a long break. */
  cycles: number;
  /** Start the next segment by itself when one ends. */
  autoContinue: boolean;
}

export const DEFAULT_POMODORO: PomodoroConfig = {
  focusMs: 25 * 60_000,
  shortMs: 5 * 60_000,
  longMs: 15 * 60_000,
  cycles: 4,
  autoContinue: true,
};

export const MAX_COUNTDOWN_MS = 99 * 3_600_000 + 59 * 60_000 + 59_000;

export interface TimerState {
  kind: TimerKind;
  phase: TimerPhase;
  segment: Segment;
  /** Focus sessions finished in this run of the pomodoro (drives the long break). */
  completed: number;
  /** Epoch ms the current run segment began; `null` unless running. */
  startedAt: number | null;
  /** Time already counted before `startedAt`. */
  accumulatedMs: number;
  /** Length of the current segment (pomodoro, countdown); `null` for a stopwatch. */
  durationMs: number | null;
}

export function segmentMs(seg: Segment, cfg: PomodoroConfig): number {
  return seg === 'focus' ? cfg.focusMs : seg === 'short' ? cfg.shortMs : cfg.longMs;
}

/** A timer at rest. `countdownMs` is the length of a countdown. */
export function initialState(
  kind: TimerKind,
  cfg: PomodoroConfig = DEFAULT_POMODORO,
  countdownMs = 5 * 60_000,
): TimerState {
  return {
    kind,
    phase: 'idle',
    segment: 'focus',
    completed: 0,
    startedAt: null,
    accumulatedMs: 0,
    durationMs:
      kind === 'pomodoro' ? cfg.focusMs : kind === 'countdown' ? clampCountdown(countdownMs) : null,
  };
}

export function clampCountdown(ms: number): number {
  if (!Number.isFinite(ms)) return 1000;
  return Math.min(MAX_COUNTDOWN_MS, Math.max(1000, Math.round(ms)));
}

/** Time counted so far in the current segment. */
export function elapsedMs(s: TimerState, now: number): number {
  const live = s.phase === 'running' && s.startedAt !== null ? Math.max(0, now - s.startedAt) : 0;
  const total = s.accumulatedMs + live;
  return s.durationMs === null ? total : Math.min(total, s.durationMs);
}

/** Time left in the current segment; `null` for a stopwatch. */
export function remainingMs(s: TimerState, now: number): number | null {
  return s.durationMs === null ? null : Math.max(0, s.durationMs - elapsedMs(s, now));
}

export function start(s: TimerState, now: number): TimerState {
  if (s.phase === 'running') return s;
  // Starting a finished timer starts it over; starting an idle or paused one carries on.
  const base = s.phase === 'done' ? { ...s, accumulatedMs: 0 } : s;
  return { ...base, phase: 'running', startedAt: now };
}

export function pause(s: TimerState, now: number): TimerState {
  if (s.phase !== 'running') return s;
  return { ...s, phase: 'paused', accumulatedMs: elapsedMs(s, now), startedAt: null };
}

export function reset(s: TimerState, cfg: PomodoroConfig = DEFAULT_POMODORO): TimerState {
  return initialState(
    s.kind,
    cfg,
    s.kind === 'countdown' ? (s.durationMs ?? undefined) : undefined,
  );
}

/** Change the length of a countdown (only while it is not running). */
export function setCountdown(s: TimerState, ms: number): TimerState {
  if (s.kind !== 'countdown' || s.phase === 'running') return s;
  return { ...s, phase: 'idle', accumulatedMs: 0, startedAt: null, durationMs: clampCountdown(ms) };
}

/** The segment after `s.segment` in a pomodoro cycle. */
export function nextSegment(
  segment: Segment,
  completedAfter: number,
  cfg: PomodoroConfig,
): Segment {
  if (segment !== 'focus') return 'focus';
  return completedAfter % cfg.cycles === 0 ? 'long' : 'short';
}

export interface Advance {
  state: TimerState;
  /** Segments that finished while advancing, oldest first (for notifications). */
  finished: Segment[];
}

/**
 * Settle a running timer against `now`: end the segment that ran out and, for a pomodoro with
 * `autoContinue`, begin the next one at the exact moment the last ended (so a tab that slept through a break
 * wakes up in the right place). A countdown or a pomodoro without auto-continue stops at `done`.
 */
export function advance(
  s: TimerState,
  now: number,
  cfg: PomodoroConfig = DEFAULT_POMODORO,
): Advance {
  let state = s;
  const finished: Segment[] = [];
  // A day of one-second segments would be 86 400 turns; the bound stops a corrupt state, not a real one.
  for (let guard = 0; guard < 2000; guard++) {
    if (state.phase !== 'running' || state.durationMs === null || state.startedAt === null) break;
    const left = state.durationMs - state.accumulatedMs - (now - state.startedAt);
    if (left > 0) break;
    const endedAt = state.startedAt + (state.durationMs - state.accumulatedMs);
    finished.push(state.segment);
    if (state.kind !== 'pomodoro') {
      state = { ...state, phase: 'done', startedAt: null, accumulatedMs: state.durationMs };
      break;
    }
    const completed = state.segment === 'focus' ? state.completed + 1 : state.completed;
    const segment = nextSegment(state.segment, completed, cfg);
    const next: TimerState = {
      ...state,
      segment,
      completed,
      durationMs: segmentMs(segment, cfg),
      accumulatedMs: 0,
      startedAt: endedAt,
      phase: 'running',
    };
    if (!cfg.autoContinue) {
      state = { ...next, phase: 'paused', startedAt: null };
      break;
    }
    state = next;
  }
  return { state, finished };
}

/** Skip to the next pomodoro segment now (keeps running if it was running). */
export function skip(
  s: TimerState,
  now: number,
  cfg: PomodoroConfig = DEFAULT_POMODORO,
): TimerState {
  if (s.kind !== 'pomodoro') return s;
  const completed = s.segment === 'focus' ? s.completed + 1 : s.completed;
  const segment = nextSegment(s.segment, completed, cfg);
  const running = s.phase === 'running';
  return {
    ...s,
    segment,
    completed,
    durationMs: segmentMs(segment, cfg),
    accumulatedMs: 0,
    startedAt: running ? now : null,
    phase: running ? 'running' : 'paused',
  };
}

/** "25:00", "1:02:03", and for a stopwatch "1:02.3" when `tenths` is set. */
export function formatClock(ms: number, tenths = false): string {
  const total = Math.max(0, Math.floor(ms / 100)) / 10;
  const whole = Math.floor(total);
  const h = Math.floor(whole / 3600);
  const m = Math.floor((whole % 3600) / 60);
  const sec = whole % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  const t = tenths ? `.${Math.floor((ms % 1000) / 100)}` : '';
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}${t}` : `${pad(m)}:${pad(sec)}${t}`;
}

/** Parse "90", "5:30", "1:02:03", "10m", "1h30m", "45s" into milliseconds; `null` when it is not a time. */
export function parseClock(text: string): number | null {
  const s = text.trim().toLowerCase();
  if (!s) return null;
  if (/^\d+(?::\d{1,2}){1,2}$/.test(s)) {
    const parts = s.split(':').map(Number);
    const [a = 0, b = 0, c] = parts;
    const secs = parts.length === 3 ? a * 3600 + b * 60 + (c ?? 0) : a * 60 + b;
    return secs > 0 ? secs * 1000 : null;
  }
  const m = /^(?:(\d+)\s*h)?\s*(?:(\d+)\s*m(?:in)?)?\s*(?:(\d+)\s*s)?$/.exec(s);
  if (m && (m[1] || m[2] || m[3])) {
    const secs = Number(m[1] ?? 0) * 3600 + Number(m[2] ?? 0) * 60 + Number(m[3] ?? 0);
    return secs > 0 ? secs * 1000 : null;
  }
  if (/^\d+$/.test(s)) {
    // A bare number is minutes, as every kitchen timer reads it.
    const mins = Number(s);
    return mins > 0 ? mins * 60_000 : null;
  }
  return null;
}

/** A line for a voice channel's status ("Focus 24:12"), at most 500 characters. */
export function statusLine(
  s: TimerState,
  now: number,
  labels: Record<Segment | 'countdown' | 'stopwatch', string>,
): string {
  const left = remainingMs(s, now);
  const name =
    s.kind === 'pomodoro'
      ? labels[s.segment]
      : s.kind === 'countdown'
        ? labels.countdown
        : labels.stopwatch;
  const clock = formatClock(left ?? elapsedMs(s, now));
  return s.phase === 'paused'
    ? `${name} ${clock} (paused)`.slice(0, 500)
    : `${name} ${clock}`.slice(0, 500);
}

/** The compact form that goes over the wire to a voice channel. */
export interface WireTimer {
  kind: TimerKind;
  phase: 'running' | 'paused';
  segment: Segment;
  /** Epoch ms the running segment started (running only). */
  startedMs: number | null;
  accumulatedMs: number;
  durationMs: number | null;
  completed: number;
}

export function toWire(s: TimerState): WireTimer | null {
  if (s.phase !== 'running' && s.phase !== 'paused') return null;
  return {
    kind: s.kind,
    phase: s.phase,
    segment: s.segment,
    startedMs: s.startedAt,
    accumulatedMs: s.accumulatedMs,
    durationMs: s.durationMs,
    completed: s.completed,
  };
}

export function fromWire(w: WireTimer): TimerState {
  return {
    kind: w.kind,
    phase: w.phase,
    segment: w.segment,
    completed: w.completed,
    startedAt: w.phase === 'running' ? w.startedMs : null,
    accumulatedMs: w.accumulatedMs,
    durationMs: w.durationMs,
  };
}

/** Defensive parse of a gateway payload; `null` when it is not a timer. */
export function parseWire(v: unknown): WireTimer | null {
  if (!v || typeof v !== 'object') return null;
  const o = v as Record<string, unknown>;
  const kinds = ['pomodoro', 'stopwatch', 'countdown'];
  const segs = ['focus', 'short', 'long'];
  const num = (x: unknown) => (typeof x === 'number' && Number.isFinite(x) && x >= 0 ? x : null);
  if (typeof o.kind !== 'string' || !kinds.includes(o.kind)) return null;
  if (o.phase !== 'running' && o.phase !== 'paused') return null;
  const segment =
    typeof o.segment === 'string' && segs.includes(o.segment) ? (o.segment as Segment) : 'focus';
  const accumulatedMs = num(o.accumulatedMs);
  if (accumulatedMs === null) return null;
  const durationMs = o.durationMs === null ? null : num(o.durationMs);
  if (o.kind !== 'stopwatch' && durationMs === null) return null;
  const startedMs = o.startedMs === null || o.startedMs === undefined ? null : num(o.startedMs);
  if (o.phase === 'running' && startedMs === null) return null;
  return {
    kind: o.kind as TimerKind,
    phase: o.phase,
    segment,
    startedMs,
    accumulatedMs,
    durationMs,
    completed: num(o.completed) ?? 0,
  };
}
