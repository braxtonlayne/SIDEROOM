import createClient, { type ClientOptions } from 'openapi-fetch';
import type { paths } from './generated/schema';

export type { components, paths } from './generated/schema';

export interface RetryOptions {
  /** Extra attempts after a 429 response. */
  retries?: number;
  /** Upper bound for a single wait, in milliseconds. */
  maxWaitMs?: number;
  /** Injectable for tests. */
  sleep?: (ms: number) => Promise<void>;
}

type FetchFn = (input: Request) => Promise<Response>;

function waitMs(res: Response, maxWaitMs: number): number {
  const secs = Number(res.headers.get('retry-after') ?? res.headers.get('x-ratelimit-reset-after'));
  const ms = Number.isFinite(secs) && secs > 0 ? secs * 1000 : 1000;
  return Math.min(ms, maxWaitMs);
}

/** Wrap `fetch` so a 429 is retried after the server-provided delay. */
export function retryingFetch(base: FetchFn = (r) => fetch(r), opts: RetryOptions = {}): FetchFn {
  const { retries = 2, maxWaitMs = 10_000, sleep = (ms) => new Promise((r) => setTimeout(r, ms)) } =
    opts;
  return async (request) => {
    for (let attempt = 0; ; attempt++) {
      // A Request body can be read once, so each attempt gets its own copy.
      const res = await base(request.clone());
      if (res.status !== 429 || attempt >= retries) return res;
      await sleep(waitMs(res, maxWaitMs));
    }
  };
}

export interface ApiClientOptions extends Omit<ClientOptions, 'fetch'> {
  retry?: RetryOptions;
  fetch?: FetchFn;
}

/**
 * Typed client for `/api/v1`. Sends the session cookie and retries 429s.
 * Mutations additionally need the `X-SR-CSRF` header (`GET /sr/csrf`).
 */
export function createApiClient({ retry, fetch: base, ...rest }: ApiClientOptions = {}) {
  return createClient<paths>({
    baseUrl: '/api/v1',
    credentials: 'include',
    ...rest,
    fetch: retryingFetch(base, retry),
  });
}
