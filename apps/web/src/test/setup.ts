import '@testing-library/jest-dom/vitest';
import { cleanup, configure } from '@testing-library/react';
import '../i18n/messages/en.rest';
import { setMarkdownEngine, testFallbackEngine } from '../lounge/markdown';
import { server } from './server';

// jsdom replaces AbortController/AbortSignal with its own classes, and Node's fetch/Request (undici)
// reject any `signal` that is not its own AbortSignal. Node 22's undici skips that check, but Node 24's
// does a brand check that ignores `Symbol.hasInstance`, so anything carrying a jsdom signal throws
// "Expected signal to be an instance of AbortSignal". Strip jsdom signals from Request and fetch and
// apply their abort semantics ourselves.
const NodeRequest = globalThis.Request;
const foreignSignals = new WeakMap<Request, AbortSignal>();

class BridgedRequest extends NodeRequest {
  constructor(input: RequestInfo | URL, init?: RequestInit) {
    const signal = init?.signal;
    if (signal) {
      const rest: RequestInit = { ...init };
      delete rest.signal;
      super(input, rest);
      foreignSignals.set(this, signal);
    } else {
      super(input, init);
    }
  }
}
globalThis.Request = BridgedRequest as typeof Request;

// MSW 3 intercepts Node's fetch at the socket and attributes a socket to the `fetch` call whose async context
// opened it. Under load, undici's connection pool sometimes opens the socket for a request outside that
// context (when it resumes its queue after another connection closed, for example after a component cancelled
// a mocked body). MSW then does not recognise the request and lets it through to the real network, and the
// test sees "fetch failed" (ECONNREFUSED on localhost:3000, where nothing ever listens under vitest) for a
// request it did mock; this was the p13entrance hero upload flake. Such a request never reached a handler,
// so sending a body-less one again is safe; it is retried twice at most.
const escapedMsw = (e: unknown): boolean => {
  const cause = (e as { cause?: { code?: unknown } } | null)?.cause;
  return e instanceof TypeError && cause?.code === 'ECONNREFUSED';
};

function retryEscaped(
  inner: typeof fetch,
  input: RequestInfo | URL,
  init: RequestInit | undefined,
  left = 2,
): Promise<Response> {
  const method = (
    init?.method ?? (input instanceof NodeRequest ? input.method : 'GET')
  ).toUpperCase();
  const replayable =
    (method === 'GET' || method === 'HEAD') && !(input instanceof NodeRequest && input.bodyUsed);
  return inner(input, init).catch((e: unknown) => {
    if (left > 0 && replayable && escapedMsw(e)) return retryEscaped(inner, input, init, left - 1);
    throw e;
  });
}

function bridgeJsdomSignals() {
  const raw = globalThis.fetch;
  const inner = ((input: RequestInfo | URL, init?: RequestInit) =>
    retryEscaped(raw, input, init)) as typeof fetch;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const signal =
      init?.signal ?? (input instanceof NodeRequest ? foreignSignals.get(input) : undefined);
    if (!signal) return inner(input, init);
    const rest: RequestInit = { ...init };
    delete rest.signal;
    if (signal.aborted) return Promise.reject(signal.reason);
    return new Promise<Response>((resolve, reject) => {
      const onAbort = () => reject(signal.reason);
      signal.addEventListener('abort', onAbort, { once: true });
      inner(input, rest)
        .then(resolve, reject)
        .finally(() => signal.removeEventListener('abort', onAbort));
    });
  }) as typeof fetch;
}

// Unit tests use the TS stand-in so the WASM bundle is never loaded under vitest.
setMarkdownEngine(testFallbackEngine);

// Routes are code-split (XC-PERF-001): the first render of a lazy route waits for a cold module transform,
// which can exceed the 1 s default under load. Real failures still fail, just after 5 s.
configure({ asyncUtilTimeout: 5000 });

window.scrollTo = () => {};
Element.prototype.scrollIntoView = () => {};

beforeAll(() => {
  server.listen({ onUnhandledFrame: 'error' });
  bridgeJsdomSignals();
});
afterEach(() => {
  cleanup();
  server.resetHandlers();
  sessionStorage.clear();
});
afterAll(() => server.close());
