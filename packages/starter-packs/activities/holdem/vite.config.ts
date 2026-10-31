import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const here = fileURLToPath(new URL('.', import.meta.url));

// The Activity is a plain static bundle (index.html at the root, allow-listed file types only). `vite build`
// writes it to ./dist with the bundle manifest sideroom.json at its root (the `datachannel` capability);
// scripts/build-activities.mjs zips dist for the developer portal.
export default defineConfig({
  root: here,
  base: './',
  plugins: [
    {
      name: 'sideroom-manifest',
      generateBundle() {
        this.emitFile({
          type: 'asset',
          fileName: 'sideroom.json',
          source: readFileSync(new URL('./sideroom.json', import.meta.url), 'utf8'),
        });
      },
    },
  ],
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
