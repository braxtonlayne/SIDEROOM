// Retry with exponential backoff and jitter for the lazy tagger (MUS-IDX-001).
import { ApiError } from '../../api/client';

/** A bucket answered with a status the tagger did not expect. */
export class HttpError extends Error {
  constructor(
    public readonly status: number,
    message?: string,
  ) {
    super(message ?? `http ${status}`);
  }
}

/** Worth trying again: the network, a throttled or failing server or bucket. Not a parse error, not a 4xx. */
export function isTransient(e: unknown): boolean {
  if (e instanceof ApiError) return e.status === 429 || e.status >= 500;
  if (e instanceof HttpError) return e.status === 429 || e.status >= 500;
  return e instanceof TypeError; // fetch rejects with a TypeError when the network fails
}

export interface BackoffOptions {
  /** Tries in all (default 4). */
  attempts?: number;
  baseMs?: number;
  maxMs?: number;
  sleep?: (ms: number) => Promise<void>;
  /** 0 to 1; tests pin it. */
  random?: () => number;
  signal?: AbortSignal;
}

/** The pause before retry number `retry` (1 for the first retry): base * 2^(retry-1), capped, jittered 50-100%. */
export function delayFor(retry: number, baseMs: number, maxMs: number, random: number): number {
  const raw = Math.min(maxMs, baseMs * 2 ** (retry - 1));
  return Math.round(raw * (0.5 + random / 2));
}

const realSleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Run `fn`; on a transient failure wait and run it again, up to `attempts` tries; any other error ends at once. */
export async function withBackoff<T>(fn: () => Promise<T>, o: BackoffOptions = {}): Promise<T> {
  const attempts = o.attempts ?? 4;
  const sleep = o.sleep ?? realSleep;
  for (let n = 1; ; n++) {
    if (o.signal?.aborted) throw new DOMException('aborted', 'AbortError');
    try {
      return await fn();
    } catch (e) {
      if (n >= attempts || !isTransient(e)) throw e;
      const wait =
        e instanceof ApiError && e.retryAfter !== null
          ? Math.min(e.retryAfter * 1000, o.maxMs ?? 30_000)
          : delayFor(n, o.baseMs ?? 500, o.maxMs ?? 30_000, (o.random ?? Math.random)());
      await sleep(wait);
    }
  }
}
