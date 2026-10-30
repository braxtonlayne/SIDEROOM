import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isAllowed } from './allowed';
import { ALLOWED as AUTH } from './auth/ALLOWED';
import { ALLOWED as INVITE } from './invite/ALLOWED';
import { ALLOWED as LANDING } from './landing/ALLOWED';

/**
 * D-385: each public surface imports only from its ALLOWED.ts. The Vite public build enforces it on the real module
 * graph (vite/surfaceGuard.ts); this is the same rule on the relative import graph, so a leak fails here first.
 */
const SRC = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const isFile = (p: string) => existsSync(p) && statSync(p).isFile();

function resolveImport(from: string, spec: string): string | null {
  if (!spec.startsWith('.')) return null;
  const base = resolve(dirname(from), spec);
  for (const c of [base, `${base}.ts`, `${base}.tsx`, join(base, 'index.ts'), join(base, 'index.tsx')])
    if (isFile(c)) return c;
  return null;
}

function closure(entry: string): string[] {
  const seen = new Set<string>();
  const stack = [resolve(SRC, entry)];
  while (stack.length) {
    const file = stack.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);
    if (!/\.tsx?$/.test(file)) continue;
    const code = readFileSync(file, 'utf8');
    for (const m of code.matchAll(/(?:from\s+|import\s*\(\s*|import\s+)['"]([^'"]+)['"]/g)) {
      const next = resolveImport(file, m[1]!);
      if (next) stack.push(next);
    }
  }
  return [...seen].map((f) => f.slice(SRC.length + 1).split(sep).join('/'));
}

// The module the build replaces with a stub: the rest of the catalog (vite/surfaceGuard.ts).
const STUBBED = ['i18n/messages/en.rest.ts'];
const APP_DIRS =
  /^(app|lounge|cinema|library|admin|settings|store|gateway|moderation|player|music|voice|watchparty|storage|support|developers|help|profilepage)\//;

describe.each([
  ['landing', 'public/landing/main.tsx', LANDING],
  ['auth', 'public/auth/main.tsx', AUTH],
  ['invite', 'public/invite/main.tsx', INVITE],
])('the %s surface', (_name, entry, list) => {
  it('imports only modules of its ALLOWED.ts', () => {
    const outside = closure(entry).filter((f) => !STUBBED.includes(f) && !isAllowed(list, f));
    expect(outside).toEqual([]);
  });

  it('never reaches an app directory', () => {
    expect(closure(entry).filter((f) => APP_DIRS.test(f))).toEqual([]);
  });
});

describe('the surface table', () => {
  it('has an entry file and an ALLOWED list for every bundle surface', () => {
    const cfg = JSON.parse(readFileSync(resolve(SRC, '../public-surfaces.json'), 'utf8')) as {
      surfaces: { surface: string; kind: string; entry?: string; reserved?: boolean }[];
    };
    for (const s of cfg.surfaces.filter((x) => x.kind === 'bundle' && !x.reserved)) {
      expect(isFile(resolve(SRC, '..', s.entry!)), s.surface).toBe(true);
      expect(isFile(resolve(SRC, `public/${s.surface}/ALLOWED.ts`)), s.surface).toBe(true);
    }
  });
});
