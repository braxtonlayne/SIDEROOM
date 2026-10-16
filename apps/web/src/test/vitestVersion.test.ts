// The workspace has two vitest majors: apps/web asks for ^3, the packages for ^2.1.8. A web test run that
// resolves the wrong one (a hoisted or stale install, `npx vitest` from the wrong directory) used to show up as
// one baffling failure, "Cannot read properties of undefined (reading 'indexOf')" in cinema/player
// signing.test.ts, because vitest 2.1's `.rejects.toThrow('text')` throws for any rejection. The version that
// RUNS the tests cannot be read from the module graph (imports resolve from apps/web whatever the runner is), so
// this probes the behaviour that differs and says what to do.
describe('test runner', () => {
  it('is the vitest that apps/web declares (rejects.toThrow works)', async () => {
    try {
      await expect(Promise.reject(new Error('probe'))).rejects.toThrow('probe');
    } catch (e) {
      throw new Error(
        'this run uses a vitest whose `.rejects.toThrow(text)` is broken (vitest 2.x); apps/web needs vitest 3. ' +
          'Run the tests through pnpm from apps/web (pnpm --filter @sideroom/web test) or reinstall with pnpm install. ' +
          `Original error: ${(e as Error).message}`,
      );
    }
  });
});
