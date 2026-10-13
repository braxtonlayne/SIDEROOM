import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:8080',
      // The Lounge's live connection (docs/05 gateway); the WebSocket upgrade must be proxied too.
      '/gateway': { target: 'ws://localhost:8080', ws: true },
      // The SFU's signalling WebSocket (`GET /voice/ws`, docs/specs/lounge-voice-video.md section 4); media is UDP.
      '/voice': { target: 'ws://localhost:8080', ws: true },
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
