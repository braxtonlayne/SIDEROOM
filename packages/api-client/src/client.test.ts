import { describe, expect, it, vi } from 'vitest';
import { createApiClient, retryingFetch } from './index';

const json = (status: number, body: unknown, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...headers },
  });

describe('retryingFetch', () => {
  it('retries a 429 after the Retry-After delay, then succeeds', async () => {
    const base = vi
      .fn()
      .mockResolvedValueOnce(json(429, {}, { 'retry-after': '2' }))
      .mockResolvedValueOnce(json(200, { ok: true }));
    const sleep = vi.fn().mockResolvedValue(undefined);
    const f = retryingFetch(base, { sleep });
    const res = await f(new Request('http://x/api/v1/a'));
    expect(res.status).toBe(200);
    expect(sleep).toHaveBeenCalledWith(2000);
    expect(base).toHaveBeenCalledTimes(2);
  });

  it('gives up after the configured retries', async () => {
    const base = vi.fn().mockImplementation(async () => json(429, {}, { 'retry-after': '1' }));
    const f = retryingFetch(base, { retries: 1, sleep: async () => {} });
    expect((await f(new Request('http://x/a'))).status).toBe(429);
    expect(base).toHaveBeenCalledTimes(2);
  });

  it('caps long waits', async () => {
    const base = vi
      .fn()
      .mockResolvedValueOnce(json(429, {}, { 'retry-after': '600' }))
      .mockResolvedValueOnce(json(200, {}));
    const sleep = vi.fn().mockResolvedValue(undefined);
    await retryingFetch(base, { sleep, maxWaitMs: 5000 })(new Request('http://x/a'));
    expect(sleep).toHaveBeenCalledWith(5000);
  });
});

describe('createApiClient', () => {
  it('calls typed paths under /api/v1', async () => {
    const base = vi.fn().mockResolvedValue(json(200, { id: '1', auth: 'session' }));
    const api = createApiClient({ baseUrl: 'http://localhost', fetch: base });
    const { data } = await api.GET('/users/@me');
    expect(data).toEqual({ id: '1', auth: 'session' });
    expect(new URL(base.mock.calls[0]![0].url).pathname).toBe('/users/@me');
  });
});
