import { readFileSync } from 'node:fs';
import { request as httpRequest } from 'node:http';
import type { IncomingMessage } from 'node:http';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';
import {
  CSS as LEGAL_CSS,
  cssFileName,
  loadCatalog,
  renderHouseNotes,
} from '../../../scripts/render-legal.mjs';
import { NO_SCRIPT_CSP } from '../src/security/csp';

/**
 * SEC-ENT-004 / D-385 (docs/dev/entrance-isolation.md sections 3 and 9): the development counterpart of the
 * production gate. `vite dev` has one document root, so without this a developer opening `/channels` signed out
 * would see the app, not the `302` production sends. The middleware:
 *
 *  - serves the shell of a public surface (`public-surfaces.json`: landing, auth, invite, ...) for its paths;
 *  - serves `/house-notes` as the server does (web.rs::legal): the template scripts/render-legal.mjs writes into
 *    the build, rendered here from the same source on every request, the operator values from
 *    `GET /sr/compliance/public` filled in HTML-escaped, the no-script policy; and its stylesheet. In dev
 *    sr-server has no web root, so forwarding the page there answered the API's JSON 404;
 *  - for any other document navigation asks `sr-server` for the gate decision (`GET /api/v1/users/@me` with the
 *    request's cookie): a session gets `app.html`, no session gets `302 /login?next=<path>`.
 *
 * It never gates module, asset or API requests (Vite and the proxy serve those), and it is never what CI proves:
 * CI runs the built dist behind `sr-server` (`SR_E2E_WEB=server`). The four-way agreement test and that mode bound
 * the drift between this and the server.
 */

export interface SurfaceRow {
  surface: string;
  kind: 'bundle' | 'html';
  shell?: string;
  paths: string[];
  reserved?: boolean;
}

export interface SurfaceTable {
  appShell?: string;
  surfaces: SurfaceRow[];
  publicFiles?: string[];
}

/** Caddy matcher syntax as used by public-surfaces.json: a trailing `*` matches any depth, nothing else is special. */
export function pathMatches(pattern: string, pathname: string): boolean {
  if (pattern.endsWith('*')) {
    const head = pattern.slice(0, -1);
    if (head.endsWith('/')) return pathname === head.slice(0, -1) || pathname.startsWith(head);
    return pathname.startsWith(head);
  }
  return pathname === pattern;
}

/** The surface a path belongs to, or undefined (reserved surfaces without a shell are skipped by the caller). */
export function matchSurface(table: SurfaceTable, pathname: string): SurfaceRow | undefined {
  return table.surfaces.find((s) => s.paths.some((p) => pathMatches(p, pathname)));
}

export function isPublicFile(table: SurfaceTable, pathname: string): boolean {
  return (table.publicFiles ?? []).some((p) => pathMatches(p, pathname));
}

/**
 * The server's `next` rule (docs/dev/entrance-isolation.md section 3): one leading `/`, no `//`, no `/@`, no
 * backslash, no control character, under 2 KB, for the value as written and once percent-decoded (so
 * `/%2F%2Fevil.example` is refused). The same cases are unit-tested on the Rust side and in `enterApp.ts`.
 */
export function safeNext(path: string): string | null {
  const shapeOk = (p: string) =>
    p.startsWith('/') &&
    !p.startsWith('//') &&
    !p.startsWith('/@') &&
    p.length <= 2048 &&
    // eslint-disable-next-line no-control-regex
    !/[\\\u0000-\u001f\u007f-\u009f]/.test(p);
  if (!shapeOk(path)) return null;
  if (path.includes('%')) {
    let decoded: string;
    try {
      decoded = Buffer.from(
        path.replace(/%([0-9a-fA-F]{2})/g, (_, h: string) => String.fromCharCode(parseInt(h, 16))),
        'latin1',
      ).toString('utf8');
    } catch {
      return null;
    }
    if (!shapeOk(decoded)) return null;
  }
  return path;
}

export function loginLocation(pathAndQuery: string): string {
  const next = safeNext(pathAndQuery);
  return next && next !== '/' && next !== '/app'
    ? `/login?next=${encodeURIComponent(next)}`
    : '/login';
}

/** A navigation to a document, as the server's gate defines it: `Sec-Fetch-Mode: navigate`, or `Accept` preferring HTML. */
export function isNavigation(req: Pick<IncomingMessage, 'method' | 'headers'>): boolean {
  if (req.method !== 'GET' && req.method !== 'HEAD') return false;
  const mode = req.headers['sec-fetch-mode'];
  if (mode === 'navigate') return true;
  // A page the service worker passes on is `same-origin` (Firefox); a script's fetch (`cors`, `no-cors`) never
  // counts. The same rule as web.rs::is_navigation.
  if (mode !== undefined && mode !== 'same-origin') return false;
  return /^text\/html\b/.test(String(req.headers['accept'] ?? ''));
}

const DEV_PASS =
  /^\/(?:@|src\/|node_modules\/|__|api\/|gateway|voice|internal\/|cdn\/|activities\/)/;

/** Asks sr-server whether the request carries a session. Resolves false on any failure (fail closed). */
export function hasSession(api: URL, cookie: string | undefined): Promise<boolean> {
  return new Promise((resolve) => {
    const out = httpRequest(
      {
        hostname: api.hostname,
        port: api.port,
        path: '/api/v1/users/@me',
        method: 'GET',
        headers: { cookie: cookie ?? '', accept: 'application/json' },
        timeout: 3000,
      },
      (r) => {
        r.resume();
        resolve(r.statusCode === 200);
      },
    );
    out.on('error', () => resolve(false));
    out.on('timeout', () => {
      out.destroy();
      resolve(false);
    });
    out.end();
  });
}

/** HTML-escape a value the way web.rs::escape_html does. */
export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/** web.rs::fill_placeholders: one pass, escaped, so a value holding a placeholder is never expanded again. */
export function fillPlaceholders(template: string, country: string, contact: string): string {
  return template.replace(/{{operator_(country|contact)}}/g, (_, k: string) =>
    escapeHtml(k === 'country' ? country : contact),
  );
}

/** The operator's country and contact from sr-server's public compliance settings; empty when unset or down. */
function operatorValues(api: URL): Promise<{ country: string; contact: string }> {
  const none = { country: '', contact: '' };
  return new Promise((resolve) => {
    const out = httpRequest(
      {
        hostname: api.hostname,
        port: api.port,
        path: '/api/v1/sr/compliance/public',
        method: 'GET',
        headers: { accept: 'application/json' },
        timeout: 3000,
      },
      (r) => {
        let body = '';
        r.setEncoding('utf8');
        r.on('data', (c: string) => (body += c));
        r.on('end', () => {
          try {
            const j = JSON.parse(body) as {
              operator_country?: unknown;
              operator_contact?: unknown;
            };
            resolve({
              country: typeof j.operator_country === 'string' ? j.operator_country : '',
              contact: typeof j.operator_contact === 'string' ? j.operator_contact : '',
            });
          } catch {
            resolve(none);
          }
        });
      },
    );
    out.on('error', () => resolve(none));
    out.on('timeout', () => {
      out.destroy();
      resolve(none);
    });
    out.end();
  });
}

/** The legal page's response headers, as web.rs::legal sends them. */
export const HOUSE_NOTES_HEADERS: Record<string, string> = {
  'Content-Type': 'text/html; charset=utf-8',
  'Cache-Control': 'no-cache',
  'Content-Security-Policy': NO_SCRIPT_CSP,
  'Reporting-Endpoints': 'csp-endpoint="/internal/csp-report"',
  Vary: 'Accept-Language',
  'X-Content-Type-Options': 'nosniff',
  'X-Robots-Tag': 'noindex, nofollow',
};

/** The house notes, rendered now from the catalog (only English exists, as in the build). */
export async function renderDevHouseNotes(api: URL): Promise<string> {
  const { country, contact } = await operatorValues(api);
  return fillPlaceholders(
    renderHouseNotes(loadCatalog(), `/entrance/${cssFileName()}`),
    country,
    contact,
  );
}

export interface PublicDevOptions {
  /** Where sr-server listens (default `SR_DEV_API` or http://127.0.0.1:8080). */
  api?: string;
  /** Path of public-surfaces.json (default: next to vite.config.ts). */
  surfacesFile?: string;
}

export function publicDev(options: PublicDevOptions = {}): Plugin {
  const api = new URL(options.api ?? process.env.SR_DEV_API ?? 'http://127.0.0.1:8080');
  const file =
    options.surfacesFile ?? fileURLToPath(new URL('../public-surfaces.json', import.meta.url));
  const load = (): SurfaceTable => JSON.parse(readFileSync(file, 'utf8')) as SurfaceTable;
  return {
    name: 'sideroom-public-dev',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = req.url ?? '/';
        const pathname = url.split(/[?#]/)[0] ?? '/';
        const readOnly = req.method === 'GET' || req.method === 'HEAD';
        if (readOnly && pathname === `/entrance/${cssFileName()}`) {
          res.setHeader('Content-Type', 'text/css; charset=utf-8');
          res.setHeader('Cache-Control', 'no-cache');
          res.end(req.method === 'HEAD' ? undefined : LEGAL_CSS);
          return;
        }
        if (readOnly && pathname === '/house-notes') {
          renderDevHouseNotes(api).then(
            (html) => {
              for (const [k, v] of Object.entries(HOUSE_NOTES_HEADERS)) res.setHeader(k, v);
              res.statusCode = 200;
              res.end(req.method === 'HEAD' ? undefined : html);
            },
            (e: unknown) => next(e),
          );
          return;
        }
        if (DEV_PASS.test(pathname) || !isNavigation(req)) {
          next();
          return;
        }
        const table = load();
        if (pathname !== '/' && isPublicFile(table, pathname)) {
          next();
          return;
        }
        const surface = matchSurface(table, pathname);
        if (surface?.kind === 'bundle' && surface.shell && !surface.reserved) {
          req.url = `/${surface.shell}${url.slice(pathname.length)}`;
          next();
          return;
        }
        // Everything else is the app: the session decides.
        void hasSession(api, req.headers.cookie).then((ok) => {
          if (ok) {
            req.url = `/${table.appShell ?? 'app.html'}${url.slice(pathname.length)}`;
            next();
          } else {
            res.statusCode = 302;
            res.setHeader('Location', loginLocation(url));
            res.setHeader('Cache-Control', 'no-store');
            res.end();
          }
        });
      });
    },
  };
}
