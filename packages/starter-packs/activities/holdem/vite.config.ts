import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const here = fileURLToPath(new URL('.', import.meta.url));

// The Activity is a plain static bundle (index.html at the root, allow-listed file types only). `vite build`
// writes it to ./dist; zip the contents of dist for the developer portal / the seed command.
export default defineConfig({
  root: here,
  base: './',
  resolve: {
    alias: {
      '@sideroom/embedded-app-sdk': fileURLToPath(
        new URL('../../../embedded-app-sdk/src/index.ts', import.meta.url),
      ),
    },
  },
  server: {
    port: 5175,
    strictPort: true,
    fs: { allow: [fileURLToPath(new URL('../../../..', import.meta.url))] },
  },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022', sourcemap: false },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    poolOptions: { forks: { minForks: 1, maxForks: 2 } },
  },
});
