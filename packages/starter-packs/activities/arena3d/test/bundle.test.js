// @ts-check
// Bundle packing tests (copied from Land.io's test/bundle.test.js and extended): the rules of bundle.rs and
// the Activity CSP (tools/bundle.mjs), the zip writer, the real Arena 3D bundle with its vendored SDK and Jolt,
// and proof that the narrow Jolt rewrites are what make the vendored file pass (the checks are not weakened).
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { crc32, inflateRawSync } from 'node:zlib';
import initJolt from 'jolt-physics/wasm-compat';
import { JOLT_FILE, buildZip, checkBundle, collectFiles, main, vendorJolt } from '../tools/bundle.mjs';
import { runReplay } from '../replay/run.js';
import script from '../replay/script.js';

/** @typedef {import('../tools/bundle.mjs').BundleFile} BundleFile */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** @param {string} text */
const enc = (text) => Buffer.from(text, 'utf8');

/** A minimal valid bundle, plus `extra` files. @param {BundleFile[]} [extra] @returns {BundleFile[]} */
function bundle(extra = []) {
  return [
    { path: 'index.html', data: enc('<!doctype html>\n<script type="module" src="main.js"></script>\n') },
    { path: 'main.js', data: enc("import './a.js';\n") },
    { path: 'a.js', data: enc('export const a = 1;\n') },
    ...extra,
  ];
}

/** @param {BundleFile[]} files @param {string} pattern */
function assertRefused(files, pattern) {
  const problems = checkBundle(files);
  assert.ok(problems.length > 0, 'expected a refusal');
  assert.ok(problems.some((p) => p.includes(pattern)), `no problem names ${JSON.stringify(pattern)}:\n${problems.join('\n')}`);
}

test('a minimal valid bundle is accepted', () => {
  assert.deepEqual(checkBundle(bundle()), []);
});

test('refuses traversal, empty segments, hidden files, spaces, bad types and missing extensions', () => {
  assertRefused(bundle([{ path: '../x.js', data: enc('1') }]), '../x.js');
  assertRefused(bundle([{ path: 'a//b.js', data: enc('1') }]), 'a//b.js');
  assertRefused(bundle([{ path: '.hidden.js', data: enc('1') }]), '.hidden.js');
  assertRefused(bundle([{ path: 'a b.js', data: enc('1') }]), 'a b.js');
  assertRefused(bundle([{ path: 'x.exe', data: enc('1') }]), 'x.exe: has a file type that is not allowed');
  assertRefused(bundle([{ path: 'model.glb', data: enc('1') }]), 'model.glb: has a file type that is not allowed');
  assertRefused(bundle([{ path: 'noext', data: enc('1') }]), 'noext: has a file type that is not allowed');
});

test('refuses names that differ only by case, and a file that is also a directory prefix', () => {
  assertRefused([{ path: 'index.html', data: enc('x') }, { path: 'A.js', data: enc('1') }, { path: 'a.js', data: enc('2') }], 'A.js');
  assertRefused(bundle([{ path: 'lib.js', data: enc('1') }, { path: 'lib.js/x.js', data: enc('2') }]), 'lib.js/x.js');
});

test('refuses a bundle without index.html at the root', () => {
  assertRefused([{ path: 'main.js', data: enc('1') }], 'index.html is missing');
  assertRefused([{ path: 'sub/index.html', data: enc('<p>x</p>') }], 'index.html is missing');
});

test('refuses more than 2000 entries, more than 32 MiB and a 257-character path', () => {
  /** @type {BundleFile[]} */
  const many = [{ path: 'index.html', data: enc('x') }];
  for (let i = 0; i < 2000; i++) many.push({ path: `d${i}.txt`, data: enc('x') });
  assertRefused(many, 'entries');
  assertRefused(bundle([{ path: 'big.txt', data: Buffer.alloc(33 * 1024 * 1024) }]), '32 MiB');
  const path = `${'a'.repeat(100)}/${'b'.repeat(100)}/${'c'.repeat(52)}.js`;
  assert.equal(path.length, 257);
  assertRefused(bundle([{ path, data: enc('1') }]), 'is longer than 256 characters');
});

test('refuses inline scripts, inline handlers, external URLs, eval, missing imports and CDN scripts', () => {
  const html = (/** @type {string} */ s) => bundle([]).map((f) => (f.path === 'index.html' ? { path: f.path, data: enc(s) } : f));
  assertRefused(html('<script>alert(1)</script>'), 'index.html: has an inline script');
  assertRefused(html('<button onclick="go()">x</button>'), 'inline event handler');
  assertRefused(bundle([{ path: 'a.js', data: enc("fetch('https://api.example/x');\n") }]), 'a.js: contains an absolute external URL');
  assertRefused(bundle([{ path: 'a.js', data: enc('export const a = eval("1");\n') }]), 'eval');
  assertRefused(bundle([{ path: 'a.js', data: enc('export const f = new Function("return 1");\n') }]), 'new Function');
  assertRefused(bundle([{ path: 'main.js', data: enc("import './missing.js';\n") }]), 'main.js: imports ./missing.js, which is not in the bundle');
  assertRefused(bundle([{ path: 'main.js', data: enc("const J = await import('jolt-physics/wasm-compat');\n") }]), "imports jolt-physics/wasm-compat, which is not a './'");
  assertRefused(html('<script type="module" src="https://cdn.example/x.js"></script>'), 'external URL');
});

test('buildZip writes a readable archive that round-trips', () => {
  /** @type {BundleFile[]} */
  const files = [...bundle(), { path: 'big.txt', data: enc('abc'.repeat(5000)) }, { path: 'empty.txt', data: Buffer.alloc(0) }];
  const zip = buildZip(files);
  const end = zip.length - 22;
  assert.equal(zip.readUInt32LE(end), 0x06054b50);
  const count = zip.readUInt16LE(end + 10);
  assert.equal(count, files.length);
  let p = zip.readUInt32LE(end + 16);
  for (let i = 0; i < count; i++) {
    const file = files[i];
    const method = zip.readUInt16LE(p + 10);
    const crc = zip.readUInt32LE(p + 16);
    const csize = zip.readUInt32LE(p + 20);
    const nameLen = zip.readUInt16LE(p + 28);
    const localAt = zip.readUInt32LE(p + 42);
    assert.equal(zip.toString('utf8', p + 46, p + 46 + nameLen), file.path);
    const dataAt = localAt + 30 + zip.readUInt16LE(localAt + 26) + zip.readUInt16LE(localAt + 28);
    const raw = zip.subarray(dataAt, dataAt + csize);
    const data = method === 8 ? inflateRawSync(raw) : raw;
    assert.equal(crc32(data), crc, file.path);
    assert.deepEqual(data, file.data, file.path);
    p += 46 + nameLen;
  }
  assert.equal(p, end);
});

test('the real Arena 3D bundle passes every rule and carries the game, the replay page, Jolt, the SDK and the manifest', () => {
  const files = collectFiles(ROOT);
  assert.deepEqual(checkBundle(files), []);
  const names = new Set(files.map((f) => f.path));
  for (const need of [
    'index.html', 'main.js', 'style.css', 'game.js', 'input.js', 'spatial-voice.js',
    'sim/world.js', 'sim/jolt-loader.js', 'net/codec.js', 'net/host.js', 'net/client.js', 'net/sdk-channel.js',
    'render/webgpu.js', 'render/webgl2.js', 'assets/gltf.js', 'assets/source.js', 'assets/models/beacon.json',
    'platform/sideroom-platform.js', 'replay/replay.html', 'replay/replay-page.js', 'replay/run.js', 'replay/script.js',
    `vendor/${JOLT_FILE}`, 'vendor/jolt-physics.LICENSE.txt', 'sdk/index.js', 'sdk/data.js', 'sideroom.json', 'NOTICE.txt',
  ])
    assert.ok(names.has(need), need);
  // Node-only tools, tests and sources never travel.
  for (const n of names) assert.ok(!/\.(mjs|ts|md)$/.test(n) && !n.startsWith('test/') && !n.startsWith('tools/') && !n.startsWith('types/'), n);
  for (const f of files.filter((x) => x.path.endsWith('.js') && !x.path.startsWith('vendor/'))) {
    assert.ok(!/\bfrom\s*['"]node:|import\(\s*['"]node:/.test(f.data.toString('utf8')), `${f.path} imports a node: module`);
  }
  const replay = files.find((f) => f.path === 'replay/replay.html')?.data.toString('utf8') ?? '';
  assert.ok(replay.includes('src="replay-page.js"') && replay.includes('<output id="hash"'));
  const sdk = files.find((f) => f.path === 'sdk/sdk.js')?.data.toString('utf8') ?? '';
  assert.ok(sdk.includes("from './data.js'") && !sdk.includes(': string'), 'the SDK is transpiled');
});

test('the manifest: tier 1, the six capabilities, connect self, the reserved example, under 8 KiB', () => {
  const raw = readFileSync(join(ROOT, 'sideroom.json'));
  assert.ok(raw.length <= 8192);
  const m = JSON.parse(raw.toString('utf8'));
  assert.equal(m.tier, 'TIER_1_ORIGINAL');
  assert.deepEqual(m.capabilities, ['datachannel', 'storage', 'guest_play', 'lobby_channel', 'byos_read', 'host_handoff']);
  assert.deepEqual(m.connect, ['self']);
  assert.deepEqual(m._reserved_capabilities_example, ['ingame_voice', 'byos_write']);
  assert.ok(m.min_participants >= 1 && m.max_participants === 8);
});

test('the unmodified Jolt file would be refused: the rewrites, not a weaker check, make it pass', () => {
  const req = createRequire(join(ROOT, 'package.json'));
  const raw = readFileSync(req.resolve('jolt-physics/wasm-compat'));
  const files = [...bundle(), { path: `vendor/${JOLT_FILE}`, data: raw }];
  const problems = checkBundle(files);
  assert.ok(problems.some((p) => p.includes('absolute external URL')), problems.join('\n'));
  assert.ok(problems.some((p) => p.includes('node:module')), problems.join('\n'));
  assert.deepEqual(checkBundle([...bundle(), ...vendorJolt()]), []);
  // Only the two rewritten strings differ.
  const vendored = vendorJolt()[0].data;
  assert.ok(Math.abs(vendored.length - raw.length) < 200);
});

test('a different Jolt file (another version) stops the build until someone re-checks the rewrites', () => {
  const dir = mkdtempSync(join(tmpdir(), 'arena3d-jolt-'));
  try {
    const fake = join(dir, 'jolt-physics.wasm-compat.js');
    writeFileSync(fake, 'export default async function Jolt() {}\n');
    assert.throws(() => vendorJolt(fake), /re-check JOLT_REWRITES/i);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the vendored Jolt runs on its browser code path and gives the same replay hash as the npm package', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'arena3d-dist-'));
  try {
    const log = console.log;
    console.log = () => {};
    try {
      main(['--dir', dir]);
    } finally {
      console.log = log;
    }
    assert.ok(!process.exitCode, 'the bundle was written');
    // A separate Node process: process.type = 'renderer' is Emscripten's test for "not Node", so the vendored file
    // takes its browser start-up path (where the rewritten Node-only import is never reached).
    const url = (/** @type {string} */ p) => JSON.stringify(pathToFileURL(p).href);
    const code = [
      `import { runReplay } from ${url(join(ROOT, 'replay', 'run.js'))};`,
      `import script from ${url(join(ROOT, 'replay', 'script.js'))};`,
      "process.type = 'renderer';",
      `const { default: init } = await import(${url(join(dir, 'vendor', JOLT_FILE))});`,
      'const r = runReplay(await init(), script);',
      'console.log(JSON.stringify({ final: r.final, exact: r.finalExact }));',
    ].join('\n');
    const out = execFileSync(process.execPath, ['--input-type=module', '-e', code], { encoding: 'utf8' });
    const vendored = JSON.parse(out.trim().split('\n').pop() ?? '{}');
    const pkg = runReplay(await initJolt(), script);
    assert.equal(vendored.exact, pkg.finalExact);
    assert.equal(vendored.final, pkg.final);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('no import in the bundle climbs out of the bundle root', () => {
  const files = collectFiles(ROOT);
  const names = new Set(files.map((f) => f.path));
  for (const f of files.filter((x) => x.path.endsWith('.js') && !x.path.startsWith('vendor/'))) {
    for (const m of f.data.toString('utf8').matchAll(/(?:\bfrom\s*|\bimport\s*\(?\s*)(['"])(\.{1,2}\/[^'"\n]+)\1/g)) {
      const stack = /** @type {string[]} */ ([]);
      let escaped = false;
      for (const seg of f.path.split('/').slice(0, -1).concat(m[2].split('/'))) {
        if (seg === '..') {
          if (!stack.length) escaped = true;
          else stack.pop();
        } else if (seg !== '.') stack.push(seg);
      }
      assert.ok(!escaped, `${f.path} imports ${m[2]}, which leaves the bundle root`);
      assert.ok(names.has(stack.join('/')), `${f.path} imports ${m[2]}, which is not in the bundle`);
    }
  }
});
