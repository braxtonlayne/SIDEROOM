import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Plugin } from 'vite';

interface Surfaces {
  appShell: string;
  appPrefix: string;
  surfaces: { shell?: string; paths: string[]; reserved?: boolean; kind: string }[];
}

/** A Caddy-style matcher (`/join/*`, `/login`) as a RegExp. */
export function matcherToRegExp(pattern: string): RegExp {
  const esc = pattern.replace(/[.+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`^${esc.replace(/\*/g, '.*')}$`);
}

/**
 * `vite dev` serves several shells, as production does: the public surfaces' shells by path (from
 * `public-surfaces.json`) and `app.html` for everything else. A minimal version: the gate decision (a `302` to
 * `/login` without a session) is `sr-server`'s and is proven by the `server` harness mode, not here.
 */
export function devShells(root: string): Plugin {
  const cfg = JSON.parse(readFileSync(resolve(root, 'public-surfaces.json'), 'utf8')) as Surfaces;
  const rules = cfg.surfaces
    .filter((s) => s.kind === 'bundle' && !s.reserved && s.shell)
    .flatMap((s) => s.paths.map((p) => ({ re: matcherToRegExp(p), shell: `/${s.shell}` })));
  return {
    name: 'sideroom-dev-shells',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, _res, next) => {
        const url = req.url ?? '/';
        const path = url.split('?')[0]!;
        const wantsHtml = req.method === 'GET' && (req.headers.accept ?? '').includes('text/html');
        if (!wantsHtml || /\.[a-z0-9]+$/i.test(path) || /^\/(@|src\/|node_modules\/|api\/)/.test(path))
          return next();
        const hit = rules.find((r) => r.re.test(path));
        req.url = (hit ? hit.shell : `/${cfg.appShell}`) + url.slice(path.length);
        next();
      });
    },
  };
}
