import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { pseudoLocalePlugin } from './vite/pseudoLocale';
import { neutralTheme } from './vite/neutralTheme';
import { pwaPlugin } from './vite/pwa';
import { readSurfaces, surfaceGuard } from './vite/surfaceGuard';
import { ALLOWED as AUTH } from './src/public/auth/ALLOWED';
import { ALLOWED as INVITE } from './src/public/invite/ALLOWED';
import { ALLOWED as LANDING } from './src/public/landing/ALLOWED';

/**
 * The PUBLIC build (D-385, docs/dev/entrance-isolation.md section 2): one HTML shell per public surface, each its
 * own minimal closure, written to `dist/` with its assets under `/entrance/`. Nothing here is built from a module
 * under an app directory; `surfaceGuard` fails the build when a surface reaches one. The gated app is a second,
 * separate build (`vite.config.ts` → `dist/app.html` and `dist/app/*`); `scripts/build-web.mjs` runs both.
 */
const root = fileURLToPath(new URL('.', import.meta.url));
const bundles = readSurfaces(root).filter((s) => s.kind === 'bundle' && !s.reserved && s.shell);

export default defineConfig({
  root,
  base: '/',
  define: {
    // A shared component can drop an app-only branch at build time (`import.meta.env.SR_BUILD === 'public'`).
    'import.meta.env.SR_BUILD': JSON.stringify('public'),
    'import.meta.env.VITE_SR_PSEUDO_LOCALE': JSON.stringify(
      process.env.SR_PSEUDO_LOCALE === '1' ? 'true' : 'false',
    ),
  },
  plugins: [
    neutralTheme(),
    react(),
    surfaceGuard(root, { landing: LANDING, auth: AUTH, invite: INVITE }),
    pwaPlugin(),
    pseudoLocalePlugin(),
  ],
  build: {
    outDir: 'dist',
    // scripts/build-web.mjs empties dist once, before the first of the two builds.
    emptyOutDir: false,
    assetsDir: 'entrance',
    manifest: true,
    rollupOptions: {
      input: Object.fromEntries(bundles.map((s) => [s.surface, fileURLToPath(new URL(s.shell!, import.meta.url))])),
    },
  },
  test: { environment: 'jsdom', globals: true, setupFiles: ['./src/test/setup.ts'], css: false },
});
