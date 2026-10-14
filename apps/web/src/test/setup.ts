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

function bridgeJsdomSignals() {
  const inner = globalThis.fetch;
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
