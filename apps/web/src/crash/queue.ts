import { buildCrashCard, type CrashReport } from './allowList';
import { CRASH_QUEUE_KEY } from './enums';

/**
 * The offline crash queue (OPS-CRASH-002). A crash card that could not be sent (the browser is offline, or the server
 * answered 5xx) is kept in `localStorage` and sent on the next `online` or visible event.
 *
 * What is stored is the allow-listed card and nothing else: every entry is written from, and read back through,
 * `buildCrashCard`, so a hand-edited storage entry cannot add a field. It is bounded (at most 10 cards, none older
 * than 7 days), keyed by a fingerprint so the same crash is never queued or sent twice, and every storage call is
 * wrapped: a browser with storage off simply has no queue.
 */

export const QUEUE_MAX = 10;
export const QUEUE_MAX_AGE_MS = 7 * 864e5;

export interface QueuedCrash {
  /** Local fingerprint (commit and the top five frames, or kind and route without frames). */
  fp: string;
  /** When it was queued, Unix ms; the 7 days count from here. */
  queuedAt: number;
  report: CrashReport;
}

/** The client twin of the server's fingerprint: the commit and the top 5 frames. */
export function fingerprintOf(r: CrashReport): string {
  const top = r.frames
    .slice(0, 5)
    .map((f) => `${f.file}:${f.line}:${f.col}:${f.fn ?? ''}`)
    .join('|');
  return `${r.commit}|${r.kind}|${r.frames.length ? top : r.route}`;
}

function revive(raw: unknown): QueuedCrash | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const o = raw as { queuedAt?: unknown; report?: unknown };
  if (typeof o.queuedAt !== 'number' || !Number.isFinite(o.queuedAt)) return null;
  const rep = o.report as Partial<CrashReport> | null | undefined;
  if (typeof rep !== 'object' || rep === null) return null;
  // Rebuild through the allow-list: only its fields survive.
  const report = buildCrashCard({ ...rep, caps: { ...rep.caps, engine: rep.engine } }, o.queuedAt);
  return report ? { fp: fingerprintOf(report), queuedAt: o.queuedAt, report } : null;
}

/** The live entries (stored order, oldest first). Never throws. */
export function readQueue(now: number = Date.now()): QueuedCrash[] {
  try {
    const text = localStorage.getItem(CRASH_QUEUE_KEY);
    const parsed: unknown = text ? JSON.parse(text) : [];
    if (!Array.isArray(parsed)) return [];
    const seen = new Set<string>();
    const out: QueuedCrash[] = [];
    for (const raw of parsed) {
      const e = revive(raw);
      if (!e || seen.has(e.fp)) continue;
      if (now - e.queuedAt > QUEUE_MAX_AGE_MS || e.queuedAt - now > 864e5) continue;
      seen.add(e.fp);
      out.push(e);
    }
    return out.slice(-QUEUE_MAX);
  } catch {
    return [];
  }
}

function write(entries: QueuedCrash[]): void {
  try {
    if (entries.length === 0) localStorage.removeItem(CRASH_QUEUE_KEY);
    else
      localStorage.setItem(
        CRASH_QUEUE_KEY,
        JSON.stringify(entries.map((e) => ({ queuedAt: e.queuedAt, report: e.report }))),
      );
  } catch {
    /* storage off or full: no queue */
  }
}

/** Keep a card that could not be sent. A card already queued is not queued again; the oldest falls out past 10. */
export function enqueueCrash(report: CrashReport, now: number = Date.now()): boolean {
  const fp = fingerprintOf(report);
  const entries = readQueue(now);
  if (entries.some((e) => e.fp === fp)) return false;
  entries.push({ fp, queuedAt: now, report });
  write(entries.slice(-QUEUE_MAX));
  return true;
}

/** Forget one card (it was sent, or the server will never take it). */
export function dequeueCrash(fp: string, now: number = Date.now()): void {
  const entries = readQueue(now);
  const rest = entries.filter((e) => e.fp !== fp);
  if (rest.length !== entries.length) write(rest);
}

export function queueSize(now: number = Date.now()): number {
  return readQueue(now).length;
}

/** Drop the whole queue (tests). */
export function clearQueue(): void {
  write([]);
}

/** A failure worth retrying later: no connection (not an API answer), a 5xx, or a 429. A 4xx will never succeed. */
export function isRetryable(error: unknown): boolean {
  const status = (error as { status?: unknown } | null)?.status;
  if (typeof status !== 'number') return true;
  return status >= 500 || status === 429;
}
