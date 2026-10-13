import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { setMarkdownEngine, testFallbackEngine } from '../lounge/markdown';
import { server } from './server';

// jsdom replaces AbortController/AbortSignal with its own classes, and Node's fetch (undici) rejects
// any `init.signal` that is not its own AbortSignal. Node 22's undici skips that check, but Node 24's
// does a brand check that ignores `Symbol.hasInstance`, so every fetch carrying a jsdom signal throws
// "Expected signal to be an instance of AbortSignal". After MSW has patched fetch (so we sit in front
// of its Request construction), strip the jsdom signal and apply its abort semantics ourselves.
function bridgeJsdomSignals() {
  const inner = globalThis.fetch;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const signal = init?.signal;
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
