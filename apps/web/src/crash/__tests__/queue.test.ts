import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client';
import { buildCrashCard, type CrashReport } from '../allowList';
import { CRASH_QUEUE_KEY } from '../enums';
import { installCrashTelemetry } from '../install';
import {
  QUEUE_MAX,
  QUEUE_MAX_AGE_MS,
  clearQueue,
  enqueueCrash,
  isRetryable,
  readQueue,
} from '../queue';
import {
  MAX_REPORTS_PER_PAGE,
  flushCrashQueue,
  reportCrash,
  resetCrashState,
  setCrashSender,
} from '../report';
import { ticks } from '../ringBuffer';

const NOW = 1_800_000_000_000;

function card(i: number): CrashReport {
  const r = buildCrashCard(
    {
      commit: 'abcdef1',
      kind: 'error',
      route: 'lounge.guild',
      frames: [{ file: `/assets/app-${i}.js`, line: i + 1, col: 7, fn: `fn${i}` }],
      caps: { wasm: true, engine: 'chromium' },
      at: NOW,
      ticks: [],
    },
    NOW,
  );
  if (!r) throw new Error('fixture card rejected');
  return r;
}

const fnOf = (r: CrashReport) => r.frames[0]?.fn;

function errorWithStack(stack: string): Error {
  const e = new Error('boom');
  e.stack = stack;
  return e;
}

const apiError = (status: number) => new ApiError(status, 'x');

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(NOW);
  localStorage.clear();
  resetCrashState();
  ticks.clear();
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  resetCrashState();
  localStorage.clear();
});

describe('what is worth keeping', () => {
  it('retries a network failure, a 5xx and a 429, never a 4xx', () => {
    expect(isRetryable(new TypeError('Failed to fetch'))).toBe(true);
    expect(isRetryable(apiError(500))).toBe(true);
    expect(isRetryable(apiError(503))).toBe(true);
    expect(isRetryable(apiError(429))).toBe(true);
    expect(isRetryable(apiError(400))).toBe(false);
    expect(isRetryable(apiError(401))).toBe(false);
  });
});

describe('the queue store', () => {
  it('keeps the card once, however often it is queued', () => {
    expect(enqueueCrash(card(1), NOW)).toBe(true);
    expect(enqueueCrash(card(1), NOW)).toBe(false);
    expect(readQueue(NOW)).toHaveLength(1);
  });

  it('keeps at most 10 cards, dropping the oldest', () => {
    for (let i = 0; i < 14; i++) enqueueCrash(card(i), NOW + i);
    const q = readQueue(NOW + 20);
    expect(q).toHaveLength(QUEUE_MAX);
    expect(fnOf(q[0]!.report)).toBe('fn4');
    expect(fnOf(q[9]!.report)).toBe('fn13');
  });

  it('forgets a card after 7 days', () => {
    enqueueCrash(card(1), NOW);
    enqueueCrash(card(2), NOW + QUEUE_MAX_AGE_MS - 1000);
    const later = readQueue(NOW + QUEUE_MAX_AGE_MS + 1000);
    expect(later.map((e) => fnOf(e.report))).toEqual(['fn2']);
  });

  it('stores nothing but the allow-listed card, and reads back only that', () => {
    enqueueCrash(card(1), NOW);
    const stored = JSON.parse(localStorage.getItem(CRASH_QUEUE_KEY)!) as {
      queuedAt: number;
      report: Record<string, unknown>;
    }[];
    expect(Object.keys(stored[0]!).sort()).toEqual(['queuedAt', 'report']);
    expect(Object.keys(stored[0]!.report).sort()).toEqual(
      ['at', 'caps', 'commit', 'engine', 'frames', 'kind', 'route', 'ticks', 'v'].sort(),
    );
    // A tampered entry is rebuilt through the list (extra fields gone) or dropped (bad commit).
    stored.push({
      queuedAt: NOW,
      report: { ...card(2), message: 'token=hunter2', route: 'https://x.test/?t=secret' },
    });
    stored.push({ queuedAt: NOW, report: { ...card(3), commit: 'not hex' } });
    localStorage.setItem(CRASH_QUEUE_KEY, JSON.stringify(stored));
    const q = readQueue(NOW);
    expect(q).toHaveLength(2);
    expect(JSON.stringify(q)).not.toMatch(/hunter2|secret|message/);
  });

  it('survives garbage and unavailable storage', () => {
    localStorage.setItem(CRASH_QUEUE_KEY, '{not json');
    expect(readQueue(NOW)).toEqual([]);
    localStorage.setItem(CRASH_QUEUE_KEY, '{"a":1}');
    expect(readQueue(NOW)).toEqual([]);
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota');
    });
    expect(readQueue(NOW)).toEqual([]);
    expect(() => enqueueCrash(card(1), NOW)).not.toThrow();
    expect(() => clearQueue()).not.toThrow();
  });
});

describe('reporting while offline', () => {
  const stack = (n: number) => `Error: x\n    at f${n} (https://sr.test/assets/app-${n}.js:${n}:9)`;

  it('queues the card when the send fails and sends it on the next flush', async () => {
    const sent: CrashReport[] = [];
    let fail: unknown = new TypeError('Failed to fetch');
    setCrashSender(async (r) => {
      if (fail) throw fail;
      sent.push(r);
    });
    expect(await reportCrash(errorWithStack(stack(1)), 'error')).toBe(false);
    expect(readQueue()).toHaveLength(1);
    expect(sent).toHaveLength(0);

    // Still down: the flush keeps it.
    expect(await flushCrashQueue()).toBe(0);
    expect(readQueue()).toHaveLength(1);

    fail = null;
    expect(await flushCrashQueue()).toBe(1);
    expect(sent).toHaveLength(1);
    expect(readQueue()).toHaveLength(0);
    // Nothing left: a second flush sends nothing.
    expect(await flushCrashQueue()).toBe(0);
    expect(sent).toHaveLength(1);
  });

  it('queues after a 5xx but not after a 4xx', async () => {
    setCrashSender(async () => {
      throw apiError(502);
    });
    await reportCrash(errorWithStack(stack(1)), 'error');
    expect(readQueue()).toHaveLength(1);
    setCrashSender(async () => {
      throw apiError(400);
    });
    await reportCrash(errorWithStack(stack(2)), 'error');
    expect(readQueue()).toHaveLength(1);
  });

  it('never sends the same crash twice', async () => {
    const sent: CrashReport[] = [];
    let down = true;
    setCrashSender(async (r) => {
      if (down) throw new TypeError('offline');
      sent.push(r);
    });
    // Queued while down, delivered by the flush; the same crash reported again on this page load is not sent.
    await reportCrash(errorWithStack(stack(1)), 'error');
    expect(readQueue()).toHaveLength(1);
    down = false;
    expect(await flushCrashQueue()).toBe(1);
    expect(await reportCrash(errorWithStack(stack(1)), 'error')).toBe(false);
    await flushCrashQueue();
    expect(sent).toHaveLength(1);
    expect(readQueue()).toHaveLength(0);
  });

  it('drops a queued card whose crash was already delivered by the flush that holds it', async () => {
    const sent: CrashReport[] = [];
    setCrashSender(async (r) => {
      sent.push(r);
    });
    enqueueCrash(card(1));
    await flushCrashQueue();
    // Queued again after it went out (a second tab, a replay of storage): not sent a second time.
    enqueueCrash(card(1));
    await flushCrashQueue();
    expect(sent).toHaveLength(1);
    expect(readQueue()).toHaveLength(0);
  });

  it('respects the per-page limit and sends one at a time, oldest first', async () => {
    const inFlight: number[] = [];
    let active = 0;
    const sent: CrashReport[] = [];
    setCrashSender(async (r) => {
      active += 1;
      inFlight.push(active);
      await Promise.resolve();
      sent.push(r);
      active -= 1;
    });
    for (let i = 0; i < 8; i++) enqueueCrash(card(i), NOW + i);
    expect(await flushCrashQueue()).toBe(MAX_REPORTS_PER_PAGE);
    expect(sent).toHaveLength(MAX_REPORTS_PER_PAGE);
    expect(Math.max(...inFlight)).toBe(1);
    expect(readQueue()).toHaveLength(3);
    expect(fnOf(sent[0]!)).toBe('fn0');
  });

  it('stops at the first retryable failure and keeps the rest; drops a card the server refuses', async () => {
    const sent: (string | undefined)[] = [];
    let n = 0;
    setCrashSender(async (r) => {
      n += 1;
      if (n === 1) throw apiError(400);
      if (n === 3) throw apiError(503);
      sent.push(fnOf(r));
    });
    for (let i = 0; i < 4; i++) enqueueCrash(card(i), NOW + i);
    expect(await flushCrashQueue()).toBe(1);
    expect(sent).toEqual(['fn1']);
    // fn0 refused (gone), fn1 sent, fn2 failed (kept), fn3 untouched.
    expect(readQueue().map((e) => fnOf(e.report))).toEqual(['fn2', 'fn3']);
  });

  it('does nothing while the browser says it is offline', async () => {
    const send = vi.fn(async () => undefined);
    setCrashSender(send);
    enqueueCrash(card(1));
    vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(false);
    expect(await flushCrashQueue()).toBe(0);
    expect(send).not.toHaveBeenCalled();
    expect(readQueue()).toHaveLength(1);
  });
});

describe('Global Privacy Control', () => {
  const stack = (n: number) =>
    'Error: x' + String.fromCharCode(10) + '    at f' + n + ' (/assets/a' + n + '.js:1:2)';
  const gpc = (v: boolean | undefined) =>
    Object.defineProperty(navigator, 'globalPrivacyControl', { value: v, configurable: true });
  afterEach(() => gpc(undefined));

  it('sends nothing and stores nothing', async () => {
    const send = vi.fn(async () => undefined);
    setCrashSender(send);
    gpc(true);
    expect(await reportCrash(errorWithStack(stack(9)), 'error')).toBe(false);
    expect(send).not.toHaveBeenCalled();
    expect(enqueueCrash(card(1))).toBe(false);
    expect(localStorage.getItem(CRASH_QUEUE_KEY)).toBeNull();
  });

  it('a failed send is not queued, and an old queue is dropped unsent', async () => {
    enqueueCrash(card(1));
    const send = vi.fn(async () => {
      throw new TypeError('offline');
    });
    setCrashSender(send);
    gpc(true);
    expect(await flushCrashQueue()).toBe(0);
    expect(send).not.toHaveBeenCalled();
    expect(readQueue()).toHaveLength(0);
  });

  it('reports normally when the signal is absent or false', async () => {
    const send = vi.fn(async () => undefined);
    setCrashSender(send);
    gpc(false);
    expect(await reportCrash(errorWithStack(stack(9)), 'error')).toBe(true);
    expect(send).toHaveBeenCalledTimes(1);
  });
});

describe('the page hooks', () => {
  it('flush on online and on becoming visible, and stop after uninstall', async () => {
    const send = vi.fn(async () => undefined);
    setCrashSender(send);
    const off = installCrashTelemetry({ routeName: () => 'lounge.guild' });

    enqueueCrash(card(1));
    window.dispatchEvent(new Event('online'));
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(readQueue()).toHaveLength(0);

    enqueueCrash(card(2));
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(2));

    off();
    enqueueCrash(card(3));
    window.dispatchEvent(new Event('online'));
    await new Promise((r) => setTimeout(r, 30));
    expect(send).toHaveBeenCalledTimes(2);
  });

  it('flushes cards left by an earlier page load when the app starts', async () => {
    const send = vi.fn(async () => undefined);
    setCrashSender(send);
    enqueueCrash(card(1));
    const off = installCrashTelemetry({ routeName: () => 'lounge.guild' });
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    off();
  });
});
