import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';
import { securityHeaders } from './src/security/csp';
import { pwaPlugin } from './vite/pwa';

/** Optional comma-separated extra CSP origins for self-hosters (a separate voice host, activities). */
const list = (v: string | undefined) =>
  (v ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
const extra = {
  extraConnect: list(process.env.SR_CSP_CONNECT),
  extraFrame: list(process.env.SR_CSP_FRAME),
  extraMedia: list(process.env.SR_CSP_MEDIA),
};

export default defineConfig({
  plugins: [react(), pwaPlugin()],
  server: {
    port: 5173,
    // Report-only in dev: the HMR socket and react-refresh preamble are inline, and a header must never
    // break a dev or e2e session. The enforced policy is on `vite preview` and in Caddyfile.prod.
    headers: securityHeaders({ ...extra, dev: true }),
    proxy: {
      '/api': 'http://localhost:8080',
      // The Lounge's live connection (docs/05 gateway); the WebSocket upgrade must be proxied too.
      '/gateway': { target: 'ws://localhost:8080', ws: true },
    },
  },
  preview: {
    // Mirrors production: enforced CSP. `vite preview` is what the PWA and CSP checks run against.
    headers: securityHeaders(extra),
    proxy: {
      '/api': 'http://localhost:8080',
      '/gateway': { target: 'ws://localhost:8080', ws: true },
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
});
