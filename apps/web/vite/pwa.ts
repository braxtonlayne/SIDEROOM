import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Plugin } from 'vite';

/**
 * Finishes `public/sw.js` after the bundle is written (XC-PWA-002): injects the build id and the list of
 * entry assets to precache, taken from the built `index.html` so it always matches the hashed file names.
 * Runs for `vite build` only; the dev server serves the placeholder worker, which is never registered.
 */
export function pwaPlugin(): Plugin {
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
      const dist = join(root, outDir);
      const swPath = join(dist, 'sw.js');
      const indexPath = join(dist, 'index.html');
      if (!existsSync(swPath) || !existsSync(indexPath)) return;
      const html = readFileSync(indexPath, 'utf8');
      const urls = precacheFromHtml(html);
      for (const extra of ['/manifest.webmanifest', '/offline.css', '/icons/icon-192.png'])
        urls.add(extra);
      const list = [...urls].sort();
      const build = createHash('sha256')
        .update(html)
        .update(list.join('\n'))
        .digest('hex')
        .slice(0, 12);
      const sw = readFileSync(swPath, 'utf8')
        .replace('__SR_BUILD__', build)
        .replace('/*__SR_PRECACHE__*/ []', JSON.stringify(list));
      writeFileSync(swPath, sw);
    },
  };
}

/** Same-origin, root-relative assets the shell needs to start: scripts, styles, preloads, fonts. */
export function precacheFromHtml(html: string): Set<string> {
  const out = new Set<string>();
  const re = /\b(?:src|href)="(\/[^"#?]+\.(?:js|css|woff2|webmanifest))"/g;
  for (const m of html.matchAll(re)) out.add(m[1]!);
  return out;
}
