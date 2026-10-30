// Run with: node --test scripts
// SEC-ENT-004 (D-385, repo-checks.yml): the lists of what is public must agree. `apps/web/public-surfaces.json` is the
// source; the Caddyfile's matchers, the service worker's table (as the build finishes it) and the dev middleware's
// matcher are derived from it or written by hand next to it, and a drift between them would serve a path the check
// does not know, or cache a page the server does not serve. Parts that need a build (dist/sw.js) skip when there is none.
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tableFile = join(repo, 'apps', 'web', 'public-surfaces.json');
const caddyFile = join(repo, 'deploy', 'compose', 'caddy', 'Caddyfile.prod');
const distSw = join(repo, 'apps', 'web', 'dist', 'sw.js');
const skipAll = existsSync(tableFile) ? false : 'apps/web/public-surfaces.json does not exist yet';

const table = existsSync(tableFile)
  ? JSON.parse(readFileSync(tableFile, 'utf8'))
  : { surfaces: [] };
const bundles = table.surfaces.filter((s) => s.kind === 'bundle' && !s.reserved);

/** Caddy's path matcher: `*` matches any run of characters (a trailing `/*` matches any depth). */
const caddyMatch = (pattern, path) =>
  new RegExp(`^${pattern.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*')}$`).test(path);

/** The paths of a named matcher line: `@auth path /login /register` -> ['/login', '/register']. */
function caddyMatcher(text, name) {
  const m = new RegExp(`^\\s*@${name}\\s+path\\s+(.+)$`, 'm').exec(text);
  return m ? m[1].trim().split(/\s+/) : null;
}

const sorted = (a) => [...a].sort();

test(
  'Caddyfile.prod has one matcher per public surface with the same paths',
  { skip: skipAll },
  () => {
    assert.ok(existsSync(caddyFile), 'deploy/compose/caddy/Caddyfile.prod');
    const caddy = readFileSync(caddyFile, 'utf8');
    for (const s of bundles) {
      const got = caddyMatcher(caddy, s.surface);
      assert.ok(
        got,
        `Caddyfile.prod has no "@${s.surface} path ..." matcher for the ${s.surface} surface`,
      );
      assert.deepEqual(
        sorted(got),
        sorted(s.paths),
        `${s.surface}: Caddy matcher and public-surfaces.json differ`,
      );
    }
    const legal = table.surfaces.find((s) => s.surface === 'legal');
    if (legal) {
      const got = caddyMatcher(caddy, 'legal');
      assert.ok(got, 'Caddyfile.prod has no @legal matcher');
      assert.ok(
        legal.paths.filter((p) => p.endsWith('house-notes')).every((p) => got.includes(p)),
        '@legal serves /house-notes',
      );
    }
    const files = caddyMatcher(caddy, 'publicFiles');
    assert.ok(files, 'Caddyfile.prod has no @publicFiles matcher');
    assert.deepEqual(
      sorted(files),
      sorted(table.publicFiles),
      'public files: Caddy matcher and public-surfaces.json differ',
    );
  },
);

test(
  'every sample path of a surface reaches that surface in Caddy and no other',
  { skip: skipAll },
  () => {
    if (!existsSync(caddyFile)) return;
    const caddy = readFileSync(caddyFile, 'utf8');
    for (const s of bundles) {
      const own = caddyMatcher(caddy, s.surface) ?? [];
      for (const path of s.samples ?? []) {
        assert.ok(
          own.some((p) => caddyMatch(p, path)),
          `${path} is not matched by @${s.surface}`,
        );
        for (const other of bundles.filter((o) => o !== s)) {
          const theirs = caddyMatcher(caddy, other.surface) ?? [];
          assert.ok(
            !theirs.some((p) => caddyMatch(p, path)),
            `${path} is also matched by @${other.surface}`,
          );
        }
      }
    }
  },
);

test(
  'the app is behind the proxy: no try_files fallback, nothing named for an app path',
  { skip: skipAll },
  () => {
    if (!existsSync(caddyFile)) return;
    const caddy = readFileSync(caddyFile, 'utf8').replace(/^\s*#.*$/gm, '');
    assert.ok(
      !/try_files/.test(caddy),
      'a try_files fallback would serve the app shell without a session',
    );
    for (const word of ['/channels', '/lounge', '/cinema', '/library', '/admin', '/settings']) {
      assert.ok(
        !caddy.includes(word),
        `${word} must not appear in the Caddyfile (the gate lives in sr-server)`,
      );
    }
  },
);

test(
  'the finished service worker routes every public sample to its shell and nothing else',
  { skip: skipAll },
  (t) => {
    if (!existsSync(distSw)) return t.skip('no apps/web/dist/sw.js (run the web build first)');
    const text = readFileSync(distSw, 'utf8');
    if (text.includes('__SR_BUILD__')) return t.skip('dist/sw.js is the unfinished template');
    // apps/web is `"type": "module"`, so require() would load sw.js as ESM and its CommonJS export would be lost.
    // Evaluate it as the classic script it is, with only a `module` to export through (no `self`: no listeners).
    const sandbox = { module: { exports: {} }, URL };
    runInNewContext(text, sandbox, { filename: distSw });
    const sw = sandbox.module.exports;
    assert.equal(typeof sw.surfaceFor, 'function', 'sw.js exports surfaceFor for tests');
    for (const s of bundles) {
      for (const path of s.samples ?? []) {
        const hit = sw.surfaceFor(path);
        assert.ok(hit, `sw.js does not know ${path} (${s.surface})`);
        assert.equal(
          hit.shell,
          s.shell,
          `sw.js sends ${path} to ${hit.shell}, expected ${s.shell}`,
        );
      }
    }
    for (const path of [
      '/channels',
      '/channels/1/2',
      '/app',
      '/admin',
      '/help',
      '/settings',
      '/nonexistent-xyz',
    ]) {
      assert.equal(sw.surfaceFor(path), null, `sw.js treats ${path} as public`);
    }
  },
);

test('the dist copy of the table equals the source', { skip: skipAll }, (t) => {
  const copy = join(repo, 'apps', 'web', 'dist', 'surfaces.json');
  if (!existsSync(copy)) return t.skip('no apps/web/dist/surfaces.json (run the web build first)');
  assert.deepEqual(JSON.parse(readFileSync(copy, 'utf8')), table);
});
