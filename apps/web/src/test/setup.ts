import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { server } from './server';

window.scrollTo = () => {};
Element.prototype.scrollIntoView = () => {};

beforeAll(() => server.listen({ onUnhandledFrame: 'error' }));
afterEach(() => {
  cleanup();
  server.resetHandlers();
  sessionStorage.clear();
});
afterAll(() => server.close());
