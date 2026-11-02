// arena3d's tests (Node). Run from the repo root with
//   npx pnpm@10.12.1 --filter @sideroom/activity-arena3d test
// or here with `npx vitest run --maxWorkers=2`. One Jolt instance is ~40 MB of WASM memory, and the determinism
// gate makes two, so the pool stays at two forks.
export default {
  test: {
    include: ['test/**/*.test.js'],
    environment: 'node',
    globals: true,
    testTimeout: 180_000,
    hookTimeout: 60_000,
    pool: 'forks',
    poolOptions: { forks: { minForks: 1, maxForks: 2 } },
  },
};
