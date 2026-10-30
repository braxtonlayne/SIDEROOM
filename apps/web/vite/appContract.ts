import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Plugin } from 'vite';

/**
 * The app build's half of the dist contract (D-385, docs/dev/entrance-isolation.md sections 2 and 3):
 *
 * - `dist/app/csp.txt`: the app Content-Security-Policy, one line, sent verbatim by `sr-server` (it refuses to
 *   start when `[web] root` is set and this file is missing).
 * - `dist/app/routes.json`: the app router's route paths (`routePaths()` of `src/router.tsx`), the input of
 *   `scripts/check-entrance-isolation.mjs`'s denylist.
 * - `dist/app/manifest.webmanifest`: the gated manifest, copied from `public/app-manifest.webmanifest` when
 *   it exists (wave 30b `ent-pwa` owns the file).
 * - `dist/app.html`: the app shell moves one level up, out of the gated directory, because the server serves
 *   it for navigations and `/app/*` is only for hashed files.
 */
export function appContractPlugin(opts: { csp: string }): Plugin {
  let outDir = 'dist/app';
  let root = process.cwd();
  return {
    name: 'sideroom-app-contract',
    apply: 'build',
    configResolved(cfg) {
      outDir = join(cfg.root, cfg.build.outDir);
      root = cfg.root;
    },
    async closeBundle() {
      mkdirSync(outDir, { recursive: true });
      writeFileSync(join(outDir, 'csp.txt'), `${opts.csp}\n`);
      writeFileSync(
        join(outDir, 'routes.json'),
        `${JSON.stringify(await routePaths(root), null, 2)}\n`,
      );
      const manifest = join(root, 'public', 'app-manifest.webmanifest');
      if (existsSync(manifest))
        writeFileSync(join(outDir, 'manifest.webmanifest'), readFileSync(manifest));
      const shell = join(outDir, 'app.html');
      if (existsSync(shell)) renameSync(shell, join(outDir, '..', 'app.html'));
    },
  };
}

/** Evaluates `routePaths()` of the app router in Vite's SSR loader (the same module graph the build used). */
async function routePaths(root: string): Promise<string[]> {
  const { createServer } = await import('vite');
  const react = (await import('@vitejs/plugin-react')).default;
  const server = await createServer({
    root,
    configFile: false,
    logLevel: 'silent',
    appType: 'custom',
    server: { middlewareMode: true, hmr: false, ws: false },
    optimizeDeps: { noDiscovery: true, include: [] },
    plugins: [react()],
    define: { __SR_VERSION__: '"build"' },
    ssr: { noExternal: [/^@sideroom\//] },
  });
  try {
    const mod = (await server.ssrLoadModule('/src/router.tsx')) as { routePaths: () => string[] };
    return mod.routePaths();
  } finally {
    await server.close();
  }
}
