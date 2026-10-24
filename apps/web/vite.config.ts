import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { cspOptionsFromConfig, securityHeaders } from './src/security/csp';
import { activityHostProxy } from './vite/activityHosts';
import { pwaPlugin } from './vite/pwa';

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

export default defineConfig({
  define: { __SR_VERSION__: JSON.stringify(changelogVersion()) },
  plugins: [
    react(),
    pwaPlugin(),
    ...(process.env.VITE_ACTIVITIES_ENABLED === 'true' ? [activityHostProxy()] : []),
  ],
  // VOX-E2E-003: the E2EE media worker is a module worker that lazy-loads its WASM.
  worker: { format: 'es' },
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
