import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { setMarkdownEngine, testFallbackEngine } from '../lounge/markdown';
import { server } from './server';

// jsdom replaces AbortController/AbortSignal with its own classes, and Node's fetch (undici) checks
// `init.signal instanceof AbortSignal`. Node 22's undici skips that check (`strict: false`), but
// Node 24's does not, so MSW's interceptor throws "Expected signal ... to be an instance of
// AbortSignal" for every fetch that carries a jsdom signal. Teach Node's AbortSignal to recognise
// jsdom signals. undici only reads `aborted` and calls `addEventListener` on them, and jsdom
// implements both. Request is not overridden by jsdom, so its signal class is Node's own.
const NodeAbortSignal = new Request('http://localhost/').signal.constructor as typeof AbortSignal;
const JsdomAbortSignal = globalThis.AbortSignal;
if (NodeAbortSignal !== JsdomAbortSignal) {
  Object.defineProperty(NodeAbortSignal, Symbol.hasInstance, {
    value: (value: unknown) =>
      value instanceof JsdomAbortSignal ||
      Function.prototype[Symbol.hasInstance].call(NodeAbortSignal, value),
  });
}

// Unit tests use the TS stand-in so the WASM bundle is never loaded under vitest.
setMarkdownEngine(testFallbackEngine);

window.scrollTo = () => {};
Element.prototype.scrollIntoView = () => {};

beforeAll(() => server.listen({ onUnhandledFrame: 'error' }));
afterEach(() => {
  cleanup();
  server.resetHandlers();
  sessionStorage.clear();
});
afterAll(() => server.close());
