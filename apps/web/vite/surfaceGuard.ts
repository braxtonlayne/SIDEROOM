import { readFileSync } from 'node:fs';
import { relative, resolve, sep } from 'node:path';
import type { Plugin } from 'vite';
import { isAllowed, type AllowList } from '../src/public/allowed';

export interface Surface {
  surface: string;
  kind: 'bundle' | 'html';
  shell?: string;
  entry?: string;
  reserved?: boolean;
  budgetGzipKB: number;
}

export function readSurfaces(root: string): Surface[] {
  const file = resolve(root, 'public-surfaces.json');
  return (JSON.parse(readFileSync(file, 'utf8')) as { surfaces: Surface[] }).surfaces;
}

/**
 * D-385: the public/private line is a module list, not a convention. Two jobs:
 *
 * - `resolveId` swaps the app's "rest of the catalog" for a stub that registers nothing (the public build never
 *   carries the app's strings), and refuses a module outside every surface's list the moment it is imported.
 * - `buildEnd` walks each surface's module graph from its shell (static and dynamic imports alike) and fails the
 *   build naming the surface, the module and the importer when a module under `src/` is not in that surface's
 *   `ALLOWED.ts`. Packages outside `src/` (React, the router, `@sideroom/ui`) are not app code and are not listed.
 */
export function surfaceGuard(root: string, lists: Record<string, AllowList>): Plugin {
  const src = resolve(root, 'src') + sep;
  const stub = resolve(root, 'src/public/stubs/enRest.ts');
  const union: AllowList = Object.values(lists).flat();
  const rel = (id: string) => relative(src, id.split('?')[0]!).split(sep).join('/');
  const inSrc = (id: string) => !id.startsWith('\0') && id.split('?')[0]!.startsWith(src);
  return {
    name: 'sideroom-surface-guard',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      if (/(^|\/)messages\/en\.rest(\.ts)?$/.test(source)) return stub;
      if (!importer || !inSrc(importer)) return null;
      const r = await this.resolve(source, importer, { ...options, skipSelf: true });
      if (r && inSrc(r.id) && !/\.(test|spec)\./.test(r.id) && !isAllowed(union, rel(r.id))) {
        this.error(
          `public build: ${rel(r.id)} is not in any public surface's ALLOWED.ts (imported by ${rel(importer)}). ` +
            'A public surface may not contain app code (D-385).',
        );
      }
      return null;
    },
    buildEnd() {
      for (const s of readSurfaces(root)) {
        if (s.kind !== 'bundle' || s.reserved || !s.shell) continue;
        const list = lists[s.surface];
        if (!list) this.error(`public build: surface "${s.surface}" has no ALLOWED.ts in the guard`);
        const start = resolve(root, s.shell);
        const seen = new Set<string>();
        const stack: [string, string][] = [[start, '(shell)']];
        while (stack.length) {
          const [id, via] = stack.pop()!;
          if (seen.has(id)) continue;
          seen.add(id);
          if (inSrc(id) && !isAllowed(list!, rel(id)) && !/\.html$/.test(id)) {
            this.error(
              `public build: surface "${s.surface}" contains ${rel(id)} (via ${via}), which is not in ` +
                `src/public/${s.surface}/ALLOWED.ts`,
            );
          }
          const info = this.getModuleInfo(id);
          if (!info) continue;
          for (const next of [...info.importedIds, ...info.dynamicallyImportedIds])
            stack.push([next, inSrc(id) ? rel(id) : via]);
        }
      }
    },
  };
}
