// Sketch & Slander tests (rules, host, guest SDK, handoff, packs, bundle). Sketch & Slander is a
// workspace package (@sideroom/activity-sketch) with its own Vitest; from the repository root:
//   pnpm --filter @sideroom/activity-sketch test
// or one file while iterating, from this folder:
//   npx vitest run test/engine.test.js --maxWorkers=2
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
