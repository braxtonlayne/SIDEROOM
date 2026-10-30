// Run with: node --test scripts
// SEC-ENT-004: the isolation check against a synthetic dist, clean and with deliberate leaks. The real build is
// checked by the Web job (node scripts/check-entrance-isolation.mjs); these cases pin the rules themselves.
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, test } from 'node:test';
import {
  chunkBase,
  dynamicImports,
  findToken,
  moduleAllowed,
  nonCorePrefixes,
  readAllowedModules,
  runCheck,
  staticPrefix,
} from './check-entrance-isolation.mjs';

const roots = [];
after(() => {
  for (const r of roots) rmSync(r, { recursive: true, force: true });
});

const put = (root, rel, text) => {
  const p = join(root, rel);
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, text);
};

const SURFACES = {
  version: 1,
  appShell: 'app.html',
  appPrefix: '/app',
  surfaces: [
    {
      surface: 'landing',
      kind: 'bundle',
      shell: 'index.html',
      paths: ['/', '/join/*'],
      budgetGzipKB: 120,
    },
    {
      surface: 'profile',
      kind: 'bundle',
      shell: 'profile.html',
      paths: ['/~*'],
      budgetGzipKB: 150,
      reserved: true,
    },
    { surface: 'legal', kind: 'html', paths: ['/house-notes'], budgetGzipKB: 0 },
    { surface: 'error', kind: 'html', files: ['404.html'], paths: [], budgetGzipKB: 0 },
  ],
  publicFiles: ['/entrance/*', '/sw.js', '/manifest.webmanifest', '/robots.txt', '/offline.css'],
};

/** A clean repo root + dist; `mutate(root, dist)` may add a leak. Returns { root, dist, run }. */
function fixture(mutate) {
  const root = mkdtempSync(join(tmpdir(), 'sr-ent-'));
  roots.push(root);
  const dist = join(root, 'dist');
  put(
    root,
    'scripts/entrance-denylist.json',
    JSON.stringify({
      words: [
        { token: 'Lounge' },
        { token: 'Cinema' },
        { token: 'Library' },
        { token: 'MiniPlayer' },
      ],
    }),
  );
  put(root, 'scripts/entrance-allow.json', JSON.stringify({ allow: [] }));
  put(
    root,
    'packages/api-client/openapi.json',
    JSON.stringify({
      paths: {
        '/api/v1/sr/auth/login': { post: { tags: ['auth'] } },
        '/api/v1/guilds/{id}/channels': { get: { tags: ['guilds'] } },
      },
    }),
  );
  put(
    root,
    'apps/web/src/i18n/messages/en/entrance.ts',
    "export default {\n  'entrance.shell.home': 'Home',\n};\n",
  );
  put(
    root,
    'apps/web/src/i18n/messages/en/lounge-1.ts',
    "export default {\n  'lounge.composer.send': 'Send',\n};\n",
  );
  put(
    root,
    'apps/web/src/public/landing/ALLOWED.ts',
    "export default ['routes/entrance/Landing', 'components/Door', '@sideroom/ui'];\n",
  );
  put(dist, 'surfaces.json', JSON.stringify(SURFACES));
  put(
    dist,
    'index.html',
    '<!doctype html><html><head><title>SIDE ROOM</title><link rel="modulepreload" href="/entrance/assets/vendor-vvvvvvvv.js"><link rel="stylesheet" href="/entrance/assets/landing-cccccccc.css"></head><body><div id="root"></div><script type="module" src="/entrance/assets/landing-llllllll.js"></script></body></html>',
  );
  put(
    dist,
    'entrance/assets/landing-llllllll.js',
    'import{r as a}from"./vendor-vvvvvvvv.js";console.log("welcome",a);',
  );
  put(dist, 'entrance/assets/vendor-vvvvvvvv.js', 'export const r=1;');
  put(dist, 'entrance/assets/landing-cccccccc.css', 'body{margin:0}');
  put(
    dist,
    'entrance/.vite/manifest.json',
    JSON.stringify({
      'src/public/landing/main.tsx': {
        file: 'assets/landing-llllllll.js',
        src: 'src/public/landing/main.tsx',
        isEntry: true,
        imports: ['_vendor-vvvvvvvv.js'],
      },
      '_vendor-vvvvvvvv.js': { file: 'assets/vendor-vvvvvvvv.js' },
    }),
  );
  put(
    dist,
    'app.html',
    '<!doctype html><html><body><script type="module" src="/app/assets/main-mmmmmmmm.js"></script></body></html>',
  );
  put(dist, 'app/assets/main-mmmmmmmm.js', 'console.log("app")');
  put(dist, 'app/assets/LoungeRoot-aaaaaaaa.js', 'console.log("lounge room")');
  put(
    dist,
    'app/routes.json',
    JSON.stringify(['/', '/app', '/login', '/channels/$channelId', '/settings']),
  );
  put(
    dist,
    'legal/house-notes.en.html',
    '<!doctype html><html><body><h1>House notes</h1><p>{{operator_country}}</p></body></html>',
  );
  put(dist, '404.html', '<!doctype html><html><body>Nothing here.</body></html>');
  put(dist, 'sw.js', 'self.addEventListener("fetch",()=>{});');
  put(dist, 'manifest.webmanifest', JSON.stringify({ start_url: '/app', name: 'SIDE ROOM' }));
  put(dist, 'robots.txt', 'User-agent: *\nDisallow: /\n');
  if (mutate) mutate(root, dist);
  return { root, dist, run: () => runCheck(dist, { repoRoot: root }) };
}

const rules = (r) => r.failures.map((f) => `${f.surface}|${f.file}|${f.token}|${f.rule}`);

test('a clean build passes and lists every surface', () => {
  const r = fixture().run();
  assert.deepEqual(rules(r), []);
  assert.ok(r.ok);
  const text = r.lines.join('\n');
  assert.match(text, /ok\s+landing/);
  assert.match(text, /skip profile/);
  assert.match(text, /ok\s+legal/);
  assert.match(text, /ok\s+error/);
  assert.match(text, /ok\s+static/);
});

test('a section name in the landing bundle fails (O-7)', () => {
  for (const word of ['Lounge', 'Cinema', 'Library']) {
    const f = fixture((root, dist) =>
      put(
        dist,
        'entrance/assets/landing-llllllll.js',
        `import{r as a}from"./vendor-vvvvvvvv.js";console.log("${word} teaser",a);`,
      ),
    );
    const r = f.run();
    assert.equal(r.ok, false, word);
    assert.ok(
      r.failures.some((x) => x.surface === 'landing' && x.token === word),
      word,
    );
  }
});

test('an allow entry cannot excuse a match without a reason, and one with a reason can', () => {
  const leak = (root, dist) =>
    put(
      dist,
      'entrance/assets/landing-llllllll.js',
      'import{r as a}from"./vendor-vvvvvvvv.js";console.log("MiniPlayer",a);',
    );
  const noReason = fixture((root, dist) => {
    leak(root, dist);
    put(
      root,
      'scripts/entrance-allow.json',
      JSON.stringify({ allow: [{ surface: 'landing', token: 'MiniPlayer', reason: ' ' }] }),
    );
  }).run();
  assert.equal(noReason.ok, false);
  assert.ok(noReason.failures.some((x) => /no reason/.test(x.rule)));
  const withReason = fixture((root, dist) => {
    leak(root, dist);
    put(
      root,
      'scripts/entrance-allow.json',
      JSON.stringify({
        allow: [{ surface: 'landing', token: 'MiniPlayer', reason: 'a vendor string' }],
      }),
    );
  }).run();
  assert.deepEqual(rules(withReason), []);
});

test('an app chunk name, route path and catalog prefix are denied in a public file', () => {
  for (const [what, text] of [
    ['chunk', 'console.log("LoungeRoot")'],
    ['route', 'location.assign("/channels")'],
    ['route', 'x("/settings")'],
    ['i18n', 't("lounge.composer.send")'],
    ['api', 'fetch("/api/v1/guilds/1/channels")'],
  ]) {
    const r = fixture((root, dist) =>
      put(dist, 'entrance/assets/vendor-vvvvvvvv.js', `export const r=1;${text};`),
    ).run();
    assert.equal(r.ok, false, what + ' ' + text);
  }
  // the neutral paths are fine
  const ok = fixture((root, dist) =>
    put(
      dist,
      'entrance/assets/vendor-vvvvvvvv.js',
      'export const r=1;location.assign("/app");go("/login");',
    ),
  ).run();
  assert.deepEqual(rules(ok), []);
});

test('a dynamic import outside /entrance/ fails', () => {
  const r = fixture((root, dist) =>
    put(
      dist,
      'entrance/assets/landing-llllllll.js',
      'import{r as a}from"./vendor-vvvvvvvv.js";import("../../app/assets/main-mmmmmmmm.js");',
    ),
  ).run();
  assert.equal(r.ok, false);
  assert.ok(r.failures.some((f) => /import\(\) outside/.test(f.rule)));
});

test('a dynamic import inside /entrance/ is followed and scanned', () => {
  const r = fixture((root, dist) => {
    put(
      dist,
      'entrance/assets/landing-llllllll.js',
      'import{r as a}from"./vendor-vvvvvvvv.js";import("./late-eeeeeeee.js");',
    );
    put(dist, 'entrance/assets/late-eeeeeeee.js', 'console.log("Cinema")');
  }).run();
  assert.equal(r.ok, false);
  assert.ok(r.failures.some((f) => f.token === 'Cinema' && f.file.includes('late-')));
});

test('a source map, a preload outside /entrance/, and the app wasm fail', () => {
  const map = fixture((root, dist) =>
    put(
      dist,
      'entrance/assets/vendor-vvvvvvvv.js',
      'export const r=1;\n//# sourceMappingURL=vendor.js.map',
    ),
  ).run();
  assert.ok(map.failures.some((f) => /sourceMappingURL/.test(f.rule)));
  const preload = fixture((root, dist) =>
    put(
      dist,
      'index.html',
      '<html><head><link rel="modulepreload" href="/app/assets/main-mmmmmmmm.js"></head><body><script type="module" src="/entrance/assets/landing-llllllll.js"></script></body></html>',
    ),
  ).run();
  assert.ok(preload.failures.some((f) => /preload outside/.test(f.rule)));
  const wasm = fixture((root, dist) => put(dist, 'entrance/assets/x_bg-xxxxxxxx.wasm', 'x')).run();
  assert.ok(wasm.failures.some((f) => /\.wasm/.test(f.rule)));
});

test('over the budget fails', () => {
  const big = Array.from({ length: 200_000 }, (_, i) => `v${i}${(i * 2654435761) % 99991}`).join(
    ',',
  );
  const r = fixture((root, dist) =>
    put(dist, 'entrance/assets/vendor-vvvvvvvv.js', `export const r=[${big}];`),
  ).run();
  assert.ok(r.failures.some((f) => /over the 120 KB budget/.test(f.rule)));
});

test('a module outside ALLOWED.ts fails, through the Vite manifest', () => {
  const r = fixture((root, dist) => {
    put(
      dist,
      'entrance/.vite/manifest.json',
      JSON.stringify({
        'src/public/landing/main.tsx': {
          file: 'assets/landing-llllllll.js',
          src: 'src/public/landing/main.tsx',
          isEntry: true,
        },
        'src/lounge/LoungeRoot.tsx': {
          file: 'assets/vendor-vvvvvvvv.js',
          src: 'src/lounge/LoungeRoot.tsx',
        },
      }),
    );
  }).run();
  assert.ok(
    r.failures.some((f) => /ALLOWED\.ts/.test(f.rule) && f.token === 'src/lounge/LoungeRoot.tsx'),
  );
});

test('a plain-HTML surface with a script, a handler or a fetch fails', () => {
  for (const html of [
    '<html><body><script>1</script></body></html>',
    '<html><body><a onclick="x()">y</a></body></html>',
    '<html><body>fetch("/x")</body></html>',
  ]) {
    const r = fixture((root, dist) => put(dist, 'legal/house-notes.en.html', html)).run();
    assert.equal(r.ok, false, html);
  }
});

test('a shell that is not in the table fails', () => {
  const r = fixture((root, dist) => put(dist, 'extra.html', '<html></html>')).run();
  assert.ok(
    r.failures.some((f) => f.file === 'extra.html' && /not in public-surfaces/.test(f.rule)),
  );
});

test('a surface without a shell fails, unless it is reserved', () => {
  const r = fixture((root, dist) => {
    const t = JSON.parse(JSON.stringify(SURFACES));
    t.surfaces.push({
      surface: 'invite',
      kind: 'bundle',
      shell: 'invite.html',
      paths: ['/invite/*'],
      budgetGzipKB: 120,
    });
    put(dist, 'surfaces.json', JSON.stringify(t));
  }).run();
  assert.ok(r.failures.some((f) => f.surface === 'invite' && /shell is not in dist/.test(f.rule)));
});

test('a file shared with the app shell fails', () => {
  const r = fixture((root, dist) =>
    put(
      dist,
      'app.html',
      '<html><body><script type="module" src="/entrance/assets/vendor-vvvvvvvv.js"></script></body></html>',
    ),
  ).run();
  assert.ok(r.failures.some((f) => /also referenced from app\.html/.test(f.rule)));
});

test('the service worker and manifest are scanned', () => {
  const r = fixture((root, dist) => put(dist, 'sw.js', 'const P=["/channels","/library"];')).run();
  assert.ok(r.failures.some((f) => f.surface === 'static' && f.file === 'sw.js'));
});

test('helpers: chunk names, prefixes, dynamic imports, tokens, modules', () => {
  assert.equal(chunkBase('assets/LoungeRoot-AbC123dE.js'), 'LoungeRoot');
  assert.equal(chunkBase('assets/react-vendor-AbC123dE.js'), 'react-vendor');
  assert.equal(staticPrefix('/channels/$id/x'), '/channels');
  assert.equal(staticPrefix('/guilds/{id}/members'), '/guilds');
  assert.equal(staticPrefix('/a/'), '/a');
  assert.deepEqual(
    dynamicImports('entrance/a.js', 'import("./b.js");import("/entrance/c.js")').targets,
    ['entrance/b.js', 'entrance/c.js'],
  );
  assert.equal(dynamicImports('a.js', 'import(x)').computed, 1);
  const t = (token, kind) => ({ token, kind });
  assert.equal(findToken('xLoungex', 'xloungex', t('Lounge', 'word')), null);
  assert.equal(findToken('a Lounge b', 'a lounge b', t('Lounge', 'word')), 2);
  assert.equal(findToken('a LOUNGE b', 'a lounge b', t('Lounge', 'word')), 2);
  assert.equal(findToken('"/channels"', '"/channels"', t('/channels', 'route')), 1);
  assert.equal(findToken('"/channelsX"', '"/channelsx"', t('/channels', 'route')), null);
  assert.equal(findToken('a/channels', 'a/channels', t('/channels', 'route')), null);
  assert.ok(
    moduleAllowed('src/routes/entrance/Landing.tsx', 'landing', ['routes/entrance/Landing']),
  );
  assert.ok(
    !moduleAllowed('src/routes/entrance/Login.tsx', 'landing', ['routes/entrance/Landing']),
  );
  assert.ok(moduleAllowed('src/public/landing/main.tsx', 'landing', []));
  assert.ok(!moduleAllowed('src/public/auth/main.tsx', 'landing', []));
  assert.ok(moduleAllowed('node_modules/react/index.js', 'landing', []));
  const dir = mkdtempSync(join(tmpdir(), 'sr-ent-'));
  roots.push(dir);
  put(
    dir,
    'ALLOWED.ts',
    "// 'routes/nope'\nexport default ['routes/entrance/{Landing,Door}', \"lib/inviteCode\"];\n",
  );
  assert.deepEqual(readAllowedModules(join(dir, 'ALLOWED.ts')), [
    'routes/entrance/Landing',
    'routes/entrance/Door',
    'lib/inviteCode',
  ]);
  put(dir, 'm/entrance.ts', "export default {\n  'entrance.a.b': 'x',\n};\n");
  put(
    dir,
    'm/lounge-1.ts',
    "export default {\n  'lounge.a.b': 'x',\n  'entrance.zzz': 'y',\n  'help.x': 'z',\n};\n",
  );
  assert.deepEqual(nonCorePrefixes(join(dir, 'm')), ['help', 'lounge']);
});
