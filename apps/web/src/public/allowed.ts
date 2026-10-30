/**
 * The allow-list mechanism of the public surfaces (D-385, entrance-isolation.md section 2).
 *
 * Each surface has an `ALLOWED.ts` listing every module under `apps/web/src` that its bundle may contain. An
 * entry is a path relative to `src` without its `.ts`/`.tsx` extension (a stylesheet keeps its extension); an
 * entry ending in `/` allows a whole directory. Packages outside `src` (React, the router, `@sideroom/ui`)
 * are not listed: they are not app code. `vite.public.config.ts` fails the build on any other module, and
 * `allowed.test.ts` checks the source closure of each surface against its list.
 */
export type AllowList = readonly string[];

/** `routes/entrance/Landing.tsx` becomes `routes/entrance/Landing`; a stylesheet stays whole. */
export const normalizeModule = (rel: string): string =>
  rel
    .replace(/\\/g, '/')
    .replace(/^\.\//, '')
    .replace(/\.(tsx?|mts|jsx?)$/, '');

export function isAllowed(list: AllowList, rel: string): boolean {
  const mod = normalizeModule(rel);
  return list.some((entry) => (entry.endsWith('/') ? mod.startsWith(entry) : entry === mod));
}
