import { beforeAll } from 'vitest';

/**
 * The Lounge is a lazy route (XC-PERF-001). Under vitest its first render waits for the cold transform of
 * the whole Lounge module graph, which takes 5 to 10 s on a loaded machine. A test that renders it first
 * then spends its `findBy` budget (5 s, `src/test/setup.ts`) on the transform and fails although nothing
 * is wrong. Call this at the top level of a test file that renders `/channels/...` through the app router:
 * the graph is loaded once in a hook with its own generous timeout, so every test measures only itself.
 */
export function warmLoungeRoute(): void {
  beforeAll(async () => {
    await import('../routes/LoungeRoot');
  }, 120_000);
}
