import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    globals: true,
    poolOptions: { forks: { minForks: 1, maxForks: 2 } },
  },
});
