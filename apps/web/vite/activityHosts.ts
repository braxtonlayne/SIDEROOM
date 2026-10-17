import { request } from 'node:http';
import type { Plugin } from 'vite';

/**
 * Dev server only: forwards requests for an Activity host (`<application-id>.activities.<dev host>`, DEV-ACT-001)
 * to sr-server with the Host header kept, as the wildcard Caddy site does in production
 * (deploy/compose/caddy/activities.snippet). Chromium and Firefox resolve `*.localhost` to loopback, so with
 * `server.public_url = http://localhost:5173` an Activity iframe loads from `http://<id>.activities.localhost:5173`.
 * Turned on with `VITE_ACTIVITIES_ENABLED=true` (the browser tests do).
 */
export function activityHostProxy(target = 'http://localhost:8080'): Plugin {
  const up = new URL(target);
  return {
    name: 'sideroom-activity-hosts',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const host = req.headers.host ?? '';
        if (!/^\d+\.activities\./i.test(host)) {
          next();
          return;
        }
        const out = request(
          {
            hostname: up.hostname,
            port: up.port,
            method: req.method,
            path: req.url,
            headers: req.headers,
          },
          (r) => {
            res.writeHead(r.statusCode ?? 502, r.headers);
            r.pipe(res);
          },
        );
        out.on('error', () => {
          res.statusCode = 502;
          res.end();
        });
        req.pipe(out);
      });
    },
  };
}
