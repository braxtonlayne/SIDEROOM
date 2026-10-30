#!/usr/bin/env node
// Bundle budget gate (XC-PERF-002). Usage: node scripts/check-bundle-budget.mjs [distDir]
// Fails (exit 1) when a route's first-load JavaScript exceeds the gzip budget in bundle-budget.json.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliCompressSync, gzipSync } from 'node:zlib';
import { staticImports } from './lib/bundle.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(process.argv[2] ?? join(here, '..', 'apps', 'web', 'dist'));
const config = JSON.parse(readFileSync(join(here, 'bundle-budget.json'), 'utf8'));

if (!existsSync(join(dist, 'index.html'))) {
  console.error(`No build at ${dist}. Run: pnpm --filter @sideroom/web build`);
  process.exit(2);
}

const gz = new Map();
const gzipSize = (file) => {
  if (!gz.has(file)) gz.set(file, gzipSync(readFileSync(join(dist, file))).length);
  return gz.get(file);
};

function closure(roots) {
  const seen = new Set();
  const stack = [...roots];
  while (stack.length) {
    const f = stack.pop();
    if (seen.has(f) || !existsSync(join(dist, f))) continue;
    seen.add(f);
    stack.push(...staticImports(f, readFileSync(join(dist, f), 'utf8')));
  }
  return seen;
}

const html = readFileSync(join(dist, 'index.html'), 'utf8');
const entryRoots = [...html.matchAll(/(?:src|href)="\/(assets\/[^"]+\.js)"/g)].map((m) => m[1]);
const entrance = closure(entryRoots);

const kb = (n) => (n / 1024).toFixed(1);
const total = (files) => [...files].reduce((n, f) => n + gzipSize(f), 0);
let failed = false;
const rows = [];

const check = (name, files, maxKB) => {
  const size = total(files);
  const ok = size <= maxKB * 1024;
  if (!ok) failed = true;
  rows.push(
    `${ok ? 'ok  ' : 'FAIL'} ${name.padEnd(10)} ${kb(size).padStart(8)} KB gzip  (budget ${maxKB} KB, ${files.size} files)`,
  );
};

for (const [name, b] of Object.entries(config.budgets)) {
  if (!b.routeChunk) {
    check(name, entrance, b.maxGzipKB);
    continue;
  }
  const chunk = readdirSync(join(dist, 'assets')).find(
    (f) => f.startsWith(`${b.routeChunk}-`) && f.endsWith('.js'),
  );
  if (!chunk) {
    rows.push(
      `skip ${name.padEnd(10)} no chunk named ${b.routeChunk}-*.js (route not split or renamed)`,
    );
    failed = true;
    continue;
  }
  check(name, new Set([...entrance, ...closure([`assets/${chunk}`])]), b.maxGzipKB);
}

for (const f of readdirSync(join(dist, 'assets')).filter((x) => x.endsWith('.js'))) {
  const s = gzipSize(`assets/${f}`);
  if (s > config.singleChunkMaxGzipKB * 1024 && entrance.has(`assets/${f}`)) {
    rows.push(
      `FAIL chunk ${f} is ${kb(s)} KB gzip and loads on first paint (limit ${config.singleChunkMaxGzipKB} KB)`,
    );
    failed = true;
  }
}

// BYOS-CNV-001 (D-297, D-309): the ffmpeg.wasm core is GPL and must stay its own lazily fetched files
// (`assets/ffmpeg-core-*.js` and `.wasm`), never part of first paint: no entrance file may be the core or name it
// (a chunk that names it is the converter, which loads only when someone converts).
{
  const assets = readdirSync(join(dist, 'assets'));
  const core = assets.filter((f) => f.startsWith('ffmpeg-core'));
  const inEntrance = [...entrance].filter(
    (f) =>
      f.startsWith('assets/ffmpeg-core') ||
      readFileSync(join(dist, f), 'utf8').includes('ffmpeg-core'),
  );
  if (inEntrance.length) {
    rows.push(
      `FAIL ffmpeg     the ffmpeg.wasm core is reachable from first paint: ${inEntrance.join(', ')}`,
    );
    failed = true;
  } else if (
    core.length &&
    !(core.some((f) => f.endsWith('.js')) && core.some((f) => f.endsWith('.wasm')))
  ) {
    rows.push(
      `FAIL ffmpeg     the core is not two separate files (.js and .wasm): ${core.join(', ')}`,
    );
    failed = true;
  } else {
    rows.push(
      core.length
        ? `ok   ffmpeg     core is lazy and separate (${core.join(', ')})`
        : 'skip ffmpeg     no ffmpeg-core asset in this build',
    );
  }
}

// MUS-WASM-001 (wave 30): the sr-music-wasm tag reader (`assets/music_bg-*.wasm` and its `music-*.js` glue) is
// fetched only when the library's tagger starts: no entrance file may be it or name it, and the .wasm stays within
// `musicWasmMaxBrotliKB` (brotli, as a CDN serves it).
{
  const assets = readdirSync(join(dist, 'assets'));
  const wasm = assets.filter((f) => /^music_bg-[\w-]+\.wasm$/.test(f));
  const glue = new Set(
    assets.filter((f) => /^music-[\w-]+\.js$/.test(f)).map((f) => `assets/${f}`),
  );
  const inEntrance = [...entrance].filter(
    (f) => glue.has(f) || readFileSync(join(dist, f), 'utf8').includes('music_bg'),
  );
  const limit = config.musicWasmMaxBrotliKB ?? 600;
  if (wasm.length !== 1) {
    rows.push(`FAIL music     expected one music_bg-*.wasm in assets, found ${wasm.length}`);
    failed = true;
  } else if (inEntrance.length) {
    rows.push(
      `FAIL music     the music tag reader is reachable from first paint: ${inEntrance.join(', ')}`,
    );
    failed = true;
  } else {
    const br = brotliCompressSync(readFileSync(join(dist, 'assets', wasm[0]))).length;
    const ok = br <= limit * 1024;
    if (!ok) failed = true;
    rows.push(
      `${ok ? 'ok  ' : 'FAIL'} music     ${kb(br).padStart(8)} KB brotli (limit ${limit} KB), lazy (${wasm[0]})`,
    );
  }
}

// MUS-PLY-001, MUS-LIB-001 (wave 30, D-385): the floating player and the music library belong to the signed-in app.
// None of their code may load on first paint (the landing page, login, invite): the entrance holds only the gate's
// lazy stub and the public-path rule. Markers are string literals that survive minification.
{
  const MARKERS = [
    'sr.player.v1',
    'sr.player.resume',
    'sr-ply-',
    'sr-mlib',
    '/music/library',
    '/music/playlists',
  ];
  const hits = [...entrance].filter((f) => {
    const code = readFileSync(join(dist, f), 'utf8');
    return MARKERS.some((m) => code.includes(m));
  });
  rows.push(
    hits.length
      ? `FAIL player    player or music library code loads on first paint: ${hits.join(', ')}`
      : 'ok   player     no player or music library code in the entrance',
  );
  if (hits.length) failed = true;
}

// RDY-I18N-002: the pseudo-locales (en-XA, ar-XB) exist on dev hosts and in a build made with SR_PSEUDO_LOCALE=1
// only. A production bundle must hold neither the pseudo alphabet, the generated catalog nor the picker labels.
if (process.env.SR_PSEUDO_LOCALE === '1') {
  rows.push('skip pseudo     SR_PSEUDO_LOCALE=1 build: the pseudo-locales are expected');
} else {
  const MARKERS = ['àƀçðéƒĝĥîĵķļɱñöþǫŕšţûṽŵẋýž', 'Pseudo (accented)', 'Pseudo (right-to-left)'];
  const leaks = readdirSync(join(dist, 'assets'))
    .filter((f) => f.endsWith('.js'))
    .filter((f) => {
      const code = readFileSync(join(dist, 'assets', f), 'utf8');
      return MARKERS.some((m) => code.includes(m));
    });
  rows.push(
    leaks.length
      ? `FAIL pseudo     pseudo-locale text found in a production bundle: ${leaks.join(', ')}`
      : 'ok   pseudo     no pseudo-locale text in any chunk',
  );
  if (leaks.length) failed = true;
}

console.log(rows.join('\n'));
if (failed) {
  console.error(
    '\nBundle budget exceeded. Split the heavy import behind a dynamic import() or lazyNamed().',
  );
  process.exit(1);
}
