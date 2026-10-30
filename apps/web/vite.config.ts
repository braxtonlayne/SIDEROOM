import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { buildCsp, cspOptionsFromConfig, securityHeaders } from './src/security/csp';
import { activityHostProxy } from './vite/activityHosts';
import { pseudoLocalePlugin } from './vite/pseudoLocale';
import { appContractPlugin } from './vite/appContract';
import { publicDev } from './vite/publicDev';

/** Optional comma-separated extra CSP origins for self-hosters (a separate voice host, activities). */
const list = (v: string | undefined) =>
  (v ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
// Deployment settings mirrored from config.toml (H-27): activities.enabled, server.public_url, voice.public_url.
const fromConfig = cspOptionsFromConfig({
  publicUrl: process.env.VITE_PUBLIC_URL,
  activitiesEnabled: process.env.VITE_ACTIVITIES_ENABLED === 'true',
  voicePublicUrl: process.env.VITE_VOICE_PUBLIC_URL,
  // PRF-UNF-001: the same list the server reads at run time to offer Play here for PeerTube links.
  peertubeHosts: process.env.SR_PEERTUBE_HOSTS,
});
const extra = {
  extraConnect: [...(fromConfig.extraConnect ?? []), ...list(process.env.SR_CSP_CONNECT)],
  extraFrame: [...(fromConfig.extraFrame ?? []), ...list(process.env.SR_CSP_FRAME)],
  extraMedia: list(process.env.SR_CSP_MEDIA),
};

/**
 * RDY-ONB-003: the version the build embeds is the first `##` heading of the root CHANGELOG.md without its date
 * ("Phase 22"). The help button shows a one-time badge when it differs from the one a browser last saw.
 */
function changelogVersion(): string {
  try {
    const text = readFileSync(
      fileURLToPath(new URL('../../CHANGELOG.md', import.meta.url)),
      'utf8',
    );
    const heading = /^## (.+?)\s*$/m.exec(text)?.[1] ?? '';
    return heading.replace(/\s+-\s+\d{4}-\d{2}-\d{2}$/, '') || 'dev';
  } catch {
    return 'dev';
  }
}

/**
 * The gated APP build (D-385, docs/dev/entrance-isolation.md section 2): `app.html` and its assets, base `/app/`,
 * written to `dist/app/` (the shell is moved to `dist/app.html` by `appContractPlugin`). The public surfaces are a
 * separate build, `vite.public.config.ts`; `scripts/build-web.mjs` runs both. `vite dev` and `vitest` use this
 * file, with the base at `/`.
 */
export default defineConfig(({ command }) => ({
  base: command === 'build' ? '/app/' : '/',
  // The public files (fonts, icons, the service worker) belong to the public build; the app only links them.
  publicDir: command === 'build' ? false : 'public',
  build: {
    outDir: 'dist/app',
    emptyOutDir: true,
    assetsDir: '',
    rollupOptions: { input: { app: fileURLToPath(new URL('./app.html', import.meta.url)) } },
  },
  define: {
    __SR_VERSION__: JSON.stringify(changelogVersion()),
    // RDY-I18N-002: the pseudo-locales ship only in a build made with SR_PSEUDO_LOCALE=1 (src/i18n/runtime.ts).
    'import.meta.env.VITE_SR_PSEUDO_LOCALE': JSON.stringify(
      process.env.SR_PSEUDO_LOCALE === '1' ? 'true' : 'false',
    ),
  },
  plugins: [
    react(),
    appContractPlugin({ csp: buildCsp(extra) }),
    // SEC-ENT-004: the dev counterpart of sr-server's gate (shells by path, 302 to /login without a session).
    publicDev(),
    pseudoLocalePlugin(),
    ...(process.env.VITE_ACTIVITIES_ENABLED === 'true' ? [activityHostProxy()] : []),
  ],
  // VOX-E2E-003: the E2EE media worker is a module worker that lazy-loads its WASM.
  worker: { format: 'es' },
  // BYOS-CNV-001: ffmpeg.wasm starts its worker with new URL('./worker.js', import.meta.url), which the dev
  // pre-bundler would break. The core (@ffmpeg/core) is imported with ?url: a separate static file, never inlined.
  optimizeDeps: { exclude: ['@ffmpeg/ffmpeg', '@ffmpeg/util', '@ffmpeg/core'] },
  server: {
    port: 5173,
    // Report-only in dev: the HMR socket and react-refresh preamble are inline, and a header must never
    // break a dev or e2e session. The enforced policy is on `vite preview` and in Caddyfile.prod.
    headers: securityHeaders({ ...extra, dev: true }),
    proxy: {
      '/api': 'http://localhost:8080',
      // CSP violation reports (RDY-SEC-009).
      '/internal/csp-report': 'http://localhost:8080',
      // The Lounge's live connection (docs/05 gateway); the WebSocket upgrade must be proxied too.
      '/gateway': { target: 'ws://localhost:8080', ws: true },
      // The SFU's signalling WebSocket (`GET /voice/ws`, docs/specs/lounge-voice-video.md section 4); media is UDP.
      '/voice': { target: 'ws://localhost:8080', ws: true },
    },
  },
  preview: {
    // Mirrors production: enforced CSP. `vite preview` is what the PWA and CSP checks run against.
    headers: securityHeaders(extra),
    proxy: {
      '/api': 'http://localhost:8080',
      '/internal/csp-report': 'http://localhost:8080',
      '/gateway': { target: 'ws://localhost:8080', ws: true },
      // As in Caddyfile and Caddyfile.prod (`handle /voice/*`): the voice soak runs against the built bundle.
      '/voice': { target: 'ws://localhost:8080', ws: true },
      '/cdn': 'http://localhost:8080',
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    testTimeout: 30000,
  },
}));
