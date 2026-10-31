import { defineConfig } from 'vitest/config';

// Relative base: the bundle is served from `https://<application-id>.activities.<host>/` as a zip's contents.
export default defineConfig({
  base: './',
  build: { outDir: 'dist', target: 'es2022', sourcemap: false },
  server: { port: 5175, strictPort: true },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    poolOptions: { forks: { minForks: 1, maxForks: 2 } },
  },
});
