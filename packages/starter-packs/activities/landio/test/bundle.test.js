// @ts-check
// Bundle packing tests (ported from the standalone game's pack.test.js): the rules of bundle.rs and the Activity
// CSP (tools/bundle.mjs), the zip writer, and the real Land.io bundle with its vendored SDK.
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32, inflateRawSync } from 'node:zlib';
import { buildZip, checkBundle, collectFiles } from '../tools/bundle.mjs';

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

/**
 * The bundle with `file` added must be refused, and one problem must name `pattern`.
 * @param {BundleFile[]} files
 * @param {string} pattern
 */
function assertRefused(files, pattern) {
  const problems = checkBundle(files);
  assert.ok(problems.length > 0, 'expected a refusal');
  assert.ok(
    problems.some((p) => p.includes(pattern)),
    `no problem names ${JSON.stringify(pattern)}:\n${problems.join('\n')}`,
  );
}

test('a minimal valid bundle is accepted', () => {
  assert.deepEqual(checkBundle(bundle()), []);
});

test('refuses a traversal path', () => {
  assertRefused(bundle([{ path: '../x.js', data: enc('1') }]), '../x.js');
});

test('refuses an empty path segment', () => {
  assertRefused(bundle([{ path: 'a//b.js', data: enc('1') }]), 'a//b.js');
});

test('refuses a hidden file', () => {
  assertRefused(bundle([{ path: '.hidden.js', data: enc('1') }]), '.hidden.js');
});

test('refuses a name with a space', () => {
  assertRefused(bundle([{ path: 'a b.js', data: enc('1') }]), 'a b.js');
});

test('refuses a file type that is not allowed', () => {
  assertRefused(bundle([{ path: 'x.exe', data: enc('1') }]), 'x.exe: has a file type that is not allowed');
});

test('refuses a file with no extension', () => {
  assertRefused(bundle([{ path: 'noext', data: enc('1') }]), 'noext: has a file type that is not allowed');
});

test('refuses names that differ only by case', () => {
  /** @type {BundleFile[]} */
  const files = [
    { path: 'index.html', data: enc('x') },
    { path: 'A.js', data: enc('1') },
    { path: 'a.js', data: enc('2') },
  ];
  assertRefused(files, 'A.js');
});

test('refuses a file that is also a directory prefix', () => {
  assertRefused(bundle([{ path: 'lib.js', data: enc('1') }, { path: 'lib.js/x.js', data: enc('2') }]), 'lib.js/x.js');
});

test('refuses a bundle without index.html', () => {
  /** @type {BundleFile[]} */
  const files = [{ path: 'main.js', data: enc('1') }];
  assertRefused(files, 'index.html is missing');
});

test('refuses index.html only in a subdirectory', () => {
  /** @type {BundleFile[]} */
  const files = [{ path: 'sub/index.html', data: enc('<p>x</p>') }];
  assertRefused(files, 'index.html is missing');
});

test('refuses more than 2000 entries', () => {
  /** @type {BundleFile[]} */
  const files = [{ path: 'index.html', data: enc('x') }];
  for (let i = 0; i < 2000; i++) files.push({ path: `d${i}.txt`, data: enc('x') });
  assert.equal(files.length, 2001);
  assertRefused(files, 'entries');
});

test('refuses more than 32 MiB unpacked', () => {
  const big = Buffer.alloc(33 * 1024 * 1024);
  assertRefused(bundle([{ path: 'big.txt', data: big }]), '32 MiB');
});

test('refuses a path of 257 characters', () => {
  const path = `${'a'.repeat(100)}/${'b'.repeat(100)}/${'c'.repeat(52)}.js`;
  assert.equal(path.length, 257);
  assertRefused(bundle([{ path, data: enc('1') }]), 'is longer than 256 characters');
});

test('refuses an inline script', () => {
  assertRefused(
    bundle([]).map((f) => (f.path === 'index.html' ? { path: f.path, data: enc('<script>alert(1)</script>') } : f)),
    'index.html: has an inline script',
  );
});

test('refuses an inline event handler', () => {
  assertRefused(
    bundle([]).map((f) =>
      f.path === 'index.html' ? { path: f.path, data: enc('<button onclick="go()">x</button>') } : f,
    ),
    'inline event handler',
  );
});

test('refuses an external https URL in a .js file', () => {
  assertRefused(
    bundle([{ path: 'a.js', data: enc("fetch('https://api.example/x');\n") }]),
    'a.js: contains an absolute external URL',
  );
});

test('refuses eval in a .js file', () => {
  assertRefused(bundle([{ path: 'a.js', data: enc('export const a = eval("1");\n') }]), 'eval');
});

test('refuses an import of a file that is not in the bundle', () => {
  assertRefused(
    bundle([{ path: 'main.js', data: enc("import './missing.js';\n") }]),
    "main.js: imports ./missing.js, which is not in the bundle",
  );
});

test('refuses a script src on another origin', () => {
  assertRefused(
    bundle([]).map((f) =>
      f.path === 'index.html'
        ? { path: f.path, data: enc('<script type="module" src="https://cdn.example/x.js"></script>') }
        : f,
    ),
    'external URL',
  );
});

test('buildZip writes a readable archive that round-trips', () => {
  /** @type {BundleFile[]} */
  const files = [
    ...bundle(),
    { path: 'big.txt', data: enc('abc'.repeat(5000)) },
    { path: 'empty.txt', data: Buffer.alloc(0) },
  ];
  const zip = buildZip(files);
  assert.deepEqual(zip.subarray(0, 4), Buffer.from([0x50, 0x4b, 0x03, 0x04]));

  const end = zip.length - 22;
  assert.equal(zip.readUInt32LE(end), 0x06054b50);
  const count = zip.readUInt16LE(end + 10);
  assert.equal(count, files.length);
  let p = zip.readUInt32LE(end + 16);

  for (let i = 0; i < count; i++) {
    const file = files[i];
    assert.equal(zip.readUInt32LE(p), 0x02014b50, `central header ${i}`);
    const method = zip.readUInt16LE(p + 10);
    const crc = zip.readUInt32LE(p + 16);
    const csize = zip.readUInt32LE(p + 20);
    const usize = zip.readUInt32LE(p + 24);
    const nameLen = zip.readUInt16LE(p + 28);
    const localAt = zip.readUInt32LE(p + 42);
    assert.equal(zip.toString('utf8', p + 46, p + 46 + nameLen), file.path);

    assert.equal(zip.readUInt32LE(localAt), 0x04034b50, `local header ${i}`);
    const dataAt = localAt + 30 + zip.readUInt16LE(localAt + 26) + zip.readUInt16LE(localAt + 28);
    const raw = zip.subarray(dataAt, dataAt + csize);
    const data = method === 8 ? inflateRawSync(raw) : raw;
    assert.equal(data.length, usize, file.path);
    assert.equal(crc32(data), crc, file.path);
    assert.deepEqual(data, file.data, file.path);
    p += 46 + nameLen;
  }
  assert.equal(p, end, 'central directory ends at the end record');
});

test('the real Land.io bundle passes every rule and carries the host engine, the SDK and the manifest', () => {
  const files = collectFiles(ROOT);
  assert.deepEqual(checkBundle(files), []);
  const names = new Set(files.map((f) => f.path));
  for (const need of ['index.html', 'main.js', 'host/engine.js', 'host/arena.js', 'sdk/index.js', 'sdk/data.js', 'activity.json', 'net/sdk-transport.js'])
    assert.ok(names.has(need), need);
  // Nothing from the standalone game's server side or its secrets travelled.
  for (const bad of ['.env', 'auth.js', 'server/main.js', 'platform/discord-platform.js', 'platform/sign-in.js'])
    assert.ok(![...names].some((n) => n === bad || n.endsWith('/' + bad) || n.endsWith(bad)), bad);
  const manifest = JSON.parse(files.find((f) => f.path === 'activity.json').data.toString('utf8'));
  assert.deepEqual(manifest.capabilities, ['datachannel']);
  // The SDK is vendored transpiled: no TypeScript syntax is left, and the specifiers name .js files.
  const sdk = files.find((f) => f.path === 'sdk/sdk.js').data.toString('utf8');
  assert.ok(sdk.includes("from './data.js'") && !sdk.includes(': string'));
});

test('no import in the bundle climbs out of the bundle root on disk (browsers clamp it, Vite and file servers do not)', () => {
  const files = collectFiles(ROOT);
  const names = new Set(files.map((f) => f.path));
  for (const f of files.filter((x) => x.path.endsWith('.js'))) {
    const text = f.data.toString('utf8');
    for (const m of text.matchAll(/(?:\bfrom\s*|\bimport\s*\(?\s*)(['"])(\.{1,2}\/[^'"\n]+)\1/g)) {
      const parts = (f.path.split('/').slice(0, -1).concat(m[2].split('/'))).reduce((acc, seg) => {
        if (seg === '..') acc.push(null);
        else if (seg !== '.') acc.push(seg);
        return acc;
      }, /** @type {(string | null)[]} */ ([]));
      // Resolve '..' against what came before it.
      const stack = /** @type {string[]} */ ([]);
      let escaped = false;
      for (const p of parts) {
        if (p === null) {
          if (!stack.length) escaped = true;
          else stack.pop();
        } else stack.push(p);
      }
      assert.ok(!escaped, `${f.path} imports ${m[2]}, which leaves the bundle root`);
      assert.ok(names.has(stack.join('/')), `${f.path} imports ${m[2]}, which is not in the bundle`);
    }
  }
});
