import { describe, expect, it } from 'vitest';
import { ApiError } from '../../../api/client';
import { delayFor, HttpError, isTransient, withBackoff } from '../backoff';
import { runPool, TAG_CONCURRENCY } from '../pool';

describe('backoff', () => {
  it('doubles the pause, caps it and jitters between half and full', () => {
    expect(delayFor(1, 500, 30_000, 1)).toBe(500);
    expect(delayFor(2, 500, 30_000, 1)).toBe(1000);
    expect(delayFor(3, 500, 30_000, 1)).toBe(2000);
    expect(delayFor(20, 500, 30_000, 1)).toBe(30_000);
    expect(delayFor(3, 500, 30_000, 0)).toBe(1000);
  });

  it('retries transient failures with growing pauses and then succeeds', async () => {
    const sleeps: number[] = [];
    let calls = 0;
    const out = await withBackoff(
      async () => {
        calls++;
        if (calls < 4) throw new HttpError(503);
        return 'ok';
      },
      { sleep: async (ms) => void sleeps.push(ms), random: () => 1, baseMs: 100 },
    );
    expect(out).toBe('ok');
    expect(calls).toBe(4);
    expect(sleeps).toEqual([100, 200, 400]);
  });

  it('gives up after the attempt cap and rethrows the last error', async () => {
    let calls = 0;
    await expect(
      withBackoff(
        async () => {
          calls++;
          throw new HttpError(500);
        },
        { attempts: 3, sleep: async () => undefined },
      ),
    ).rejects.toBeInstanceOf(HttpError);
    expect(calls).toBe(3);
  });

  it('does not retry what retrying cannot fix', async () => {
    let calls = 0;
    await expect(
      withBackoff(
        async () => {
          calls++;
          throw new HttpError(404);
        },
        { sleep: async () => undefined },
      ),
    ).rejects.toBeInstanceOf(HttpError);
    expect(calls).toBe(1);
    expect(isTransient(new HttpError(429))).toBe(true);
    expect(isTransient(new TypeError('failed to fetch'))).toBe(true);
    expect(isTransient(new ApiError(502, 'bad gateway'))).toBe(true);
    expect(isTransient(new ApiError(403, 'no'))).toBe(false);
    expect(isTransient(new Error('parse'))).toBe(false);
  });

  it('obeys Retry-After from the server', async () => {
    const sleeps: number[] = [];
    let calls = 0;
    await withBackoff(
      async () => {
        if (calls++ === 0) throw new ApiError(429, 'slow down', { retry_after: 7 } as never);
      },
      { sleep: async (ms) => void sleeps.push(ms) },
    );
    expect(sleeps).toEqual([7000]);
  });
});

describe('runPool', () => {
  it('runs 4 at a time by default and never more', async () => {
    expect(TAG_CONCURRENCY).toBe(4);
    let active = 0;
    let peak = 0;
    const gates: Array<() => void> = [];
    const started: number[] = [];
    const run = runPool(
      Array.from({ length: 10 }, (_, i) => i),
      async (i) => {
        started.push(i);
        active++;
        peak = Math.max(peak, active);
        await new Promise<void>((r) => gates.push(r));
        active--;
      },
    );
    await new Promise((r) => setTimeout(r, 0));
    expect(started).toEqual([0, 1, 2, 3]);
    // Finish one: exactly one more starts.
    gates.shift()!();
    await new Promise((r) => setTimeout(r, 0));
    expect(started).toEqual([0, 1, 2, 3, 4]);
    for (let i = 0; i < 100 && (gates.length > 0 || started.length < 10 || active > 0); i++) {
      gates.shift()?.();
      await new Promise((r) => setTimeout(r, 0));
    }
    await run;
    expect(started).toHaveLength(10);
    expect(peak).toBe(4);
  });

  it('stops taking items once aborted and still rethrows an unexpected error', async () => {
    const ac = new AbortController();
    const seen: number[] = [];
    await runPool(
      [1, 2, 3, 4, 5, 6],
      async (i) => {
        seen.push(i);
        if (i === 2) ac.abort();
      },
      { concurrency: 1, signal: ac.signal },
    );
    expect(seen).toEqual([1, 2]);
    await expect(
      runPool([1, 2, 3], async (i) => {
        if (i === 2) throw new Error('boom');
      }),
    ).rejects.toThrow('boom');
  });
});
