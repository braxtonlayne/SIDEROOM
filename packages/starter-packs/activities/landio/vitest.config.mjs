// Land.io's tests (ported from the standalone game's node:test files, plus the host/guest tests). Land.io is a
// workspace package (@sideroom/activity-landio) with its own Vitest; from the repository root:
//   pnpm --filter @sideroom/activity-landio test
// or one file while iterating, from this folder:
//   npx vitest run test/modes/draft.test.js --maxWorkers=2
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
