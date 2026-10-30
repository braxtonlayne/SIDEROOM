import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import type { Plugin } from 'vite';

/** One public surface as the worker sees it: the shell it navigates to (null for a plain page) and its paths. */
export interface SwSurface {
  shell: string | null;
  paths: string[];
}

/**
 * The table of public surfaces (entrance-isolation.md section 2) used until `apps/web/public-surfaces.json` exists.
 * That file wins when present; `normalizeSurfaces` accepts its array or `{ surfaces: [...] }` form.
 */
export const DEFAULT_SURFACES: SwSurface[] = [
  { shell: 'index.html', paths: ['/', '/join/$code'] },
  {
    shell: 'auth.html',
    paths: [
      '/login',
      '/login/2fa',
      '/login/link',
      '/register',
      '/forgot',
      '/reset/$token',
      '/verify-email/$token',
      '/email-change/cancel',
      '/oauth/discord/callback',
    ],
  },
  { shell: 'invite.html', paths: ['/invite/$code', '/template/$code', '/no-invitation'] },
  { shell: null, paths: ['/house-notes'] },
];

/**
 * Finishes `public/sw.js` after the bundle is written (XC-PWA-002, SEC-ENT-002): injects the build id, the public
 * files to precache (read from every public shell that exists in the output, so it always matches the hashed
 * file names) and the table of public surfaces. Also moves the signed-in manifest out of the public root to
 * `app/manifest.webmanifest`, where only the session-gated build serves it. Runs for `vite build` only; the dev
 * server serves the placeholder worker, which is never registered.
 */
export function pwaPlugin(opts: { surfacesFile?: string } = {}): Plugin {
  let outDir = 'dist';
  let root = process.cwd();
  return {
    name: 'sideroom-pwa',
    apply: 'build',
    configResolved(cfg) {
      outDir = cfg.build.outDir;
      root = cfg.root;
    },
    writeBundle() {
      finishServiceWorker(
        join(root, outDir),
        opts.surfacesFile ?? join(root, 'public-surfaces.json'),
      );
    },
  };
}

/**
 * Fill the placeholders of `dist/sw.js` from the shells in `dist` and move the signed-in manifest. Exported so the
 * e2e specs can finish a copy of the worker template the way the build does.
 */
export function finishServiceWorker(dist: string, surfacesFile: string): void {
  moveAppManifest(dist);
  const swPath = join(dist, 'sw.js');
  if (!existsSync(swPath)) return;
  const surfaces = loadSurfaces(surfacesFile);
  const shells = surfaces
    .map((s) => s.shell)
    .filter((s): s is string => !!s && existsSync(join(dist, s)));
  if (shells.length === 0) return;
  const htmls = shells.map((s) => readFileSync(join(dist, s), 'utf8'));
  const urls = new Set<string>();
  for (const html of htmls) for (const u of precacheFromHtml(html)) urls.add(u);
  for (const extra of ['/manifest.webmanifest', '/offline.css', '/icons/icon-192.png'])
    urls.add(extra);
  const list = [...urls].sort();
  const present = surfaces.filter((s) => !s.shell || shells.includes(s.shell));
  const hash = createHash('sha256');
  for (const html of htmls) hash.update(html);
  const build = hash
    .update(list.join('\n'))
    .update(JSON.stringify(present))
    .digest('hex')
    .slice(0, 12);
  const sw = readFileSync(swPath, 'utf8')
    .replace('__SR_BUILD__', build)
    .replace('/*__SR_PRECACHE__*/ []', JSON.stringify(list))
    .replace('/*__SR_SURFACES__*/ []', JSON.stringify(present));
  writeFileSync(swPath, sw);
}

/** `dist/app-manifest.webmanifest` (copied from public/) becomes `dist/app/manifest.webmanifest`; never public. */
export function moveAppManifest(dist: string): void {
  const from = join(dist, 'app-manifest.webmanifest');
  if (!existsSync(from)) return;
  const to = join(dist, 'app', 'manifest.webmanifest');
  mkdirSync(dirname(to), { recursive: true });
  if (existsSync(to)) rmSync(to);
  renameSync(from, to);
}

/** Read the surface table, accepting `[...]` or `{ surfaces: [...] }`, `shell` or `html`, `paths` or `path`. */
export function loadSurfaces(file: string): SwSurface[] {
  if (!existsSync(file)) return DEFAULT_SURFACES;
  try {
    return normalizeSurfaces(JSON.parse(readFileSync(file, 'utf8')));
  } catch {
    return DEFAULT_SURFACES;
  }
}

export function normalizeSurfaces(raw: unknown): SwSurface[] {
  const list = Array.isArray(raw)
    ? raw
    : raw && typeof raw === 'object' && Array.isArray((raw as { surfaces?: unknown }).surfaces)
      ? (raw as { surfaces: unknown[] }).surfaces
      : [];
  const out: SwSurface[] = [];
  for (const item of list) {
    if (!item || typeof item !== 'object') continue;
    const o = item as Record<string, unknown>;
    const shellRaw = o.shell ?? o.html ?? null;
    const shell = typeof shellRaw === 'string' ? shellRaw.replace(/^\/+/, '') : null;
    const pathsRaw = Array.isArray(o.paths) ? o.paths : typeof o.path === 'string' ? [o.path] : [];
    const paths = pathsRaw.filter((p): p is string => typeof p === 'string' && p.startsWith('/'));
    if (paths.length > 0) out.push({ shell: shell || null, paths });
  }
  return out.length > 0 ? out : DEFAULT_SURFACES;
}

/** Same-origin, root-relative public files a shell needs to start: scripts, styles, preloads, fonts. */
export function precacheFromHtml(html: string): Set<string> {
  const out = new Set<string>();
  const re = /\b(?:src|href)="(\/[^"#?]+\.(?:js|css|woff2|webmanifest))"/g;
  for (const m of html.matchAll(re)) {
    // Defensive: a public shell never references signed-in files, and the worker must never precache one.
    if (m[1]!.startsWith('/app/')) continue;
    out.add(m[1]!);
  }
  return out;
}
