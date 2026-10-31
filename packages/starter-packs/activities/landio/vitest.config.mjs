// Land.io's tests (ported from the standalone game's node:test files, plus the host/guest tests). There is no
// package.json here (the starter packs are not workspace packages), so run them with the Vitest of any
// workspace package, rooted here:
//   node ../../../embedded-app-sdk/node_modules/vitest/vitest.mjs run --root . --maxWorkers=2
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
