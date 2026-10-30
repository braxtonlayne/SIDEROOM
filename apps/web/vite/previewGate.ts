import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Plugin } from 'vite';
import { isNavigation, isPublicFile, matchSurface, type SurfaceTable } from './publicDev';

/**
 * SEC-ENT-004 / D-385: for `playwright.static.config.ts`, which serves the production build with `vite preview`
 * (no API, no database, so no gate). `vite preview` knows one fallback document, but the build now has a shell per
 * public surface and an app shell. This plugin picks the file the way the server would, minus the session check:
 * a public surface's shell for its paths, the house notes template for `/house-notes`, and `app.html` for every
 * other navigation (ungated: the static suite proves the service worker, the CSP and axe on the public pages; the
 * gate itself is proven by web_gate.rs and entrance-isolation.spec.ts).
 */
export function previewGate(distDir = 'dist'): Plugin {
  return {
    name: 'sideroom-preview-gate',
    apply: 'serve',
    configurePreviewServer(server) {
      const root = join(server.config.root, distDir);
      server.middlewares.use((req, _res, next) => {
        const url = req.url ?? '/';
        const pathname = url.split(/[?#]/)[0] ?? '/';
        const tableFile = join(root, 'surfaces.json');
        if (!isNavigation(req) || !existsSync(tableFile)) {
          next();
          return;
        }
        const table = JSON.parse(readFileSync(tableFile, 'utf8')) as SurfaceTable;
        if (pathname !== '/' && isPublicFile(table, pathname)) {
          next();
          return;
        }
        const surface = matchSurface(table, pathname);
        const query = url.slice(pathname.length);
        if (surface?.kind === 'bundle' && surface.shell && existsSync(join(root, surface.shell))) {
          req.url = `/${surface.shell}${query}`;
        } else if (pathname === '/house-notes') {
          req.url = '/legal/house-notes.en.html';
        } else if (!/\.[a-z0-9]+$/i.test(pathname)) {
          req.url = `/${table.appShell ?? 'app.html'}${query}`;
        }
        next();
      });
    },
  };
}
