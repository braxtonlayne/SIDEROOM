// Run with: node --test scripts
// SEC-ENT-004 (D-385, repo-checks.yml): each public surface's source imports only what its own
// `apps/web/src/public/<surface>/ALLOWED.ts` lists. The public build's resolveId guard enforces the same at build time;
// this is the fast, toolchain-free copy that runs in repo-checks, so a stray `import '../../lounge/...'` fails a pull
// request in seconds. Skips until the public sources exist.
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, posix, relative, resolve } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { moduleAllowed, readAllowedModules } from './check-entrance-isolation.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const web = join(repo, 'apps', 'web');
const publicDir = join(web, 'src', 'public');
const tableFile = join(web, 'public-surfaces.json');
const skip =
  existsSync(publicDir) && existsSync(tableFile) ? false : 'apps/web/src/public does not exist yet';

const surfaces = existsSync(tableFile)
  ? JSON.parse(readFileSync(tableFile, 'utf8')).surfaces.filter((s) => s.kind === 'bundle')
  : [];

function sources(dir) {
  const out = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...sources(p));
    else if (
      /\.(tsx?|css)$/.test(e.name) &&
      !/\.(test|spec)\./.test(e.name) &&
      e.name !== 'ALLOWED.ts'
    ) {
      out.push(p);
    }
  }
  return out;
}

/** Module specifiers a source file imports, statically or dynamically (comments stripped). */
export function specifiers(code) {
  const text = code.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
  const out = [];
  for (const m of text.matchAll(/\b(?:import|export)\s[^'";]*?\bfrom\s*(["'])([^"']+)\1/g))
    out.push(m[2]);
  for (const m of text.matchAll(/\bimport\s*(["'])([^"']+)\1/g)) out.push(m[2]);
  for (const m of text.matchAll(/\bimport\(\s*(["'])([^"']+)\1\s*\)/g)) out.push(m[2]);
  return out;
}

/** The `src/...` path a relative specifier points at (extension and index dropped), or null for a package. */
function target(file, spec) {
  if (spec.startsWith('@/')) return `src/${spec.slice(2)}`;
  if (!spec.startsWith('.')) return null;
  const abs = resolve(dirname(file), spec);
  return toPosix(relative(web, abs));
}

const toPosix = (p) => p.split('\\').join('/');

test('a bundle surface has an ALLOWED.ts and its entry', { skip }, () => {
  for (const s of surfaces.filter((x) => !x.reserved)) {
    assert.ok(
      existsSync(join(publicDir, s.surface, 'ALLOWED.ts')),
      `src/public/${s.surface}/ALLOWED.ts`,
    );
    assert.ok(
      existsSync(join(publicDir, s.surface, 'main.tsx')),
      `src/public/${s.surface}/main.tsx`,
    );
  }
});

test('every public source imports only from its surface ALLOWED.ts', { skip }, () => {
  const problems = [];
  for (const s of surfaces) {
    const dir = join(publicDir, s.surface);
    if (!existsSync(dir) || !statSync(dir).isDirectory()) continue;
    const allowedTs = join(dir, 'ALLOWED.ts');
    if (!existsSync(allowedTs)) continue;
    const list = readAllowedModules(allowedTs);
    for (const f of sources(dir)) {
      for (const spec of specifiers(readFileSync(f, 'utf8'))) {
        const t = target(f, spec);
        if (t === null) {
          // A package: only the workspace packages are listed.
          if (
            spec.startsWith('@sideroom/') &&
            !list.some((a) => spec === a || spec.startsWith(`${a}/`))
          ) {
            problems.push(
              `${s.surface}: ${toPosix(relative(web, f))} imports ${spec}, not in ALLOWED.ts`,
            );
          }
          continue;
        }
        if (!moduleAllowed(t, s.surface, list)) {
          problems.push(
            `${s.surface}: ${toPosix(relative(web, f))} imports ${spec} (${posix.normalize(t)}), not in ALLOWED.ts`,
          );
        }
      }
    }
  }
  assert.deepEqual(problems, []);
});

test(
  'src/main.tsx no longer exists; the entries are per surface and src/app/main.tsx',
  { skip },
  () => {
    assert.ok(
      !existsSync(join(web, 'src', 'main.tsx')),
      'src/main.tsx must be gone (the entry of the whole app)',
    );
    assert.ok(existsSync(join(web, 'src', 'app', 'main.tsx')), 'src/app/main.tsx');
  },
);

test('specifiers: static, side-effect, re-export and dynamic imports', () => {
  assert.deepEqual(
    specifiers(
      "import a from './a';\nimport { b } from \"../b\";\nimport './c.css';\nexport * from './d';\nconst x = () => import('./e');\n// import z from './z';\n",
    ),
    ['./a', '../b', './d', './c.css', './e'],
  );
});
