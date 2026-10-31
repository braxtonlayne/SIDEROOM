import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    poolOptions: { forks: { minForks: 1, maxForks: 2 } },
  },
});
