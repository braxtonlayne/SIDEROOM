// The pure part of watch party synchronisation (spec section 4): the clock-offset estimate, the expected
// position and the drift policy. No timers, no DOM: `controller.ts` drives these with real or fake time.
import type { Playback } from './types';
import { t } from '../i18n';

/** |drift| below this is ignored (spec says 250 ms; 100 ms keeps any two viewers within 300 ms of each other). */
export const DRIFT_IGNORE_MS = 100;
/** Once nudging, keep going until the drift is below this (hysteresis, so the rate does not flap). */
export const DRIFT_CONVERGED_MS = 40;
/** |drift| above this is a hard seek. */
export const DRIFT_SEEK_MS = 2000;
/** The playback-rate nudge while converging. */
export const NUDGE = 0.05;
/** After a hard seek, wait this long before judging drift again (the player is still buffering). */
export const SETTLE_MS = 1500;
/** A stall shorter than this is not reported. */
export const BUFFER_REPORT_MS = 1500;

/** One request/response exchange with the server clock. */
export interface ClockSample {
  /** Server clock in the response. */
  serverMs: number;
  /** Local clock when the request was sent and when the response arrived. */
  sentMs: number;
  receivedMs: number;
}

/**
 * NTP-style offset estimate: `server = local + offset`. Each sample gives `offset = server - (sent + rtt/2)`;
 * samples with the smallest round trips are the most trustworthy, so the estimate is the median offset of the
 * lowest-RTT few of the most recent samples.
 */
export class ClockEstimator {
  private samples: { offset: number; rtt: number }[] = [];
  constructor(
    private readonly keep = 12,
    private readonly best = 3,
  ) {}

  add(s: ClockSample): void {
    const rtt = Math.max(0, s.receivedMs - s.sentMs);
    this.samples.push({ offset: s.serverMs - (s.sentMs + rtt / 2), rtt });
    if (this.samples.length > this.keep) this.samples.shift();
  }

  get ready(): boolean {
    return this.samples.length > 0;
  }

  /** Offset to add to the local clock to get server time (0 until a sample exists). */
  get offsetMs(): number {
    if (this.samples.length === 0) return 0;
    const best = [...this.samples].sort((a, b) => a.rtt - b.rtt).slice(0, this.best);
    const offs = best.map((x) => x.offset).sort((a, b) => a - b);
    const mid = Math.floor(offs.length / 2);
    return offs.length % 2 ? offs[mid]! : (offs[mid - 1]! + offs[mid]!) / 2;
  }

  /** Smallest round trip among the recent samples. */
  get rttMs(): number | null {
    return this.samples.length ? Math.min(...this.samples.map((s) => s.rtt)) : null;
  }

  serverNow(localMs: number): number {
    return localMs + this.offsetMs;
  }
}

/** Where the media should be at server time `serverNowMs` (spec: position + (now - at) * rate). */
export function expectedPositionMs(p: Playback, serverNowMs: number): number {
  let pos = p.position_ms;
  if (p.playing) pos += Math.max(0, serverNowMs - p.at_ms) * p.rate;
  if (p.duration_ms != null) pos = Math.min(pos, p.duration_ms);
  return Math.max(0, pos);
}

export type Correction =
  | { kind: 'none' }
  /** Run at `rate` (a multiple of the party rate) until converged. */
  | { kind: 'nudge'; rate: number }
  /** Back to the party rate: converged. */
  | { kind: 'settle' }
  | { kind: 'seek'; toMs: number };

/**
 * Decide what to do about `driftMs` (player position minus expected position: positive = ahead).
 * `nudging` says whether a rate nudge is already running (hysteresis). Spec: below 250 ms ignore, 250 ms to 2 s
 * adjust the rate 0.95/1.05 until converged, above 2 s seek.
 */
export function planCorrection(
  driftMs: number,
  nudging: boolean,
  baseRate: number,
  expectedMs: number,
): Correction {
  const abs = Math.abs(driftMs);
  if (abs > DRIFT_SEEK_MS) return { kind: 'seek', toMs: Math.round(expectedMs) };
  if (nudging) {
    if (abs < DRIFT_CONVERGED_MS) return { kind: 'settle' };
    return { kind: 'nudge', rate: baseRate * (driftMs > 0 ? 1 - NUDGE : 1 + NUDGE) };
  }
  if (abs < DRIFT_IGNORE_MS) return { kind: 'none' };
  return { kind: 'nudge', rate: baseRate * (driftMs > 0 ? 1 - NUDGE : 1 + NUDGE) };
}

/** "Waiting for Ana and 2 others" from the usernames of the stalled participants. */
export function waitingLabel(names: string[]): string | null {
  if (names.length === 0) return null;
  if (names.length === 1) return t('watchparty.sync.waitingOne', { a: names[0] ?? '' });
  if (names.length === 2)
    return t('watchparty.sync.waitingTwo', { a: names[0] ?? '', b: names[1] ?? '' });
  return t('watchparty.sync.waitingMany', { a: names[0] ?? '', n: names.length - 1 });
}

/** mm:ss or h:mm:ss. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}
