// VOX-CLI-002 (D-1049): check-vad-assets.mjs on a fake dist and a fake vendor folder.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { problems } from './check-vad-assets.mjs';

const MODEL = Buffer.from('fake onnx model');
const sha = (b) => createHash('sha256').update(b).digest('hex');

function put(root, rel, data) {
  mkdirSync(dirname(join(root, rel)), { recursive: true });
  writeFileSync(join(root, rel), data);
}

/** A good build plus its vendor folder; `tweak(dist, vendor)` breaks one thing. */
function world(tweak) {
  const root = mkdtempSync(join(tmpdir(), 'vadassets-'));
  const dist = join(root, 'dist');
  const vendor = join(root, 'vendor');
  put(vendor, 'silero_vad.onnx', MODEL);
  put(vendor, 'SHA256SUMS', `${sha(MODEL)} *silero_vad.onnx\n`);
  put(dist, 'app.html', '<script type="module" src="/app/app-1.js"></script>');
  put(dist, 'app/app-1.js', 'import "./dep-2.js";console.log(1)');
  put(dist, 'app/dep-2.js', 'export const a=1');
  put(dist, 'app/lazy-vad-3.js', 'const w="ort-wasm-simd-threaded-AbC.wasm"');
  put(dist, 'app/silero_vad-Zz9.onnx', MODEL);
  put(dist, 'app/ort-wasm-simd-threaded-AbC.wasm', Buffer.alloc(1_500_000, 1));
  put(dist, 'index.html', '<script src="/entrance/landing-1.js"></script>');
  put(dist, 'entrance/landing-1.js', 'console.log("landing")');
  put(dist, 'sw.js', 'self.__WB_MANIFEST=[{url:"/offline.css"}]');
  tweak?.(dist, vendor);
  return { dist, vendor, done: () => rmSync(root, { recursive: true, force: true }) };
}

const run = (tweak) => {
  const w = world(tweak);
  try {
    return problems(w.dist, w.vendor);
  } finally {
    w.done();
  }
};

const starts = (r, tag) => r.some((p) => p.startsWith(tag));

test('a good build has no problems', () => assert.deepEqual(run(), []));

test('(a) a vendored model that is not the pinned one fails', () => {
  assert.ok(
    starts(
      run((d, v) => put(v, 'silero_vad.onnx', 'tampered')),
      '(a)',
    ),
  );
});

test('(a) a missing SHA256SUMS fails', () => {
  assert.ok(
    starts(
      run((d, v) => rmSync(join(v, 'SHA256SUMS'))),
      '(a)',
    ),
  );
});

test('(b) a build without the model or the wasm fails', () => {
  assert.ok(
    starts(
      run((d) => rmSync(join(d, 'app/silero_vad-Zz9.onnx'))),
      '(b)',
    ),
  );
  assert.ok(
    starts(
      run((d) => rmSync(join(d, 'app/ort-wasm-simd-threaded-AbC.wasm'))),
      '(b)',
    ),
  );
});

test('(b) a built model that differs from the vendored one fails', () => {
  const r = run((d) => put(d, 'app/silero_vad-Zz9.onnx', 'other'));
  assert.ok(r.some((p) => p.startsWith('(b)') && p.includes('differs')));
});

test('(b) a jsep, jspi or asyncify variant fails', () => {
  for (const v of ['jsep', 'jspi', 'asyncify']) {
    const r = run((d) =>
      put(d, `app/ort-wasm-simd-threaded.${v}-Q1.wasm`, Buffer.alloc(1_500_000)),
    );
    assert.ok(starts(r, '(b)'), v);
    const m = run((d) => put(d, `app/ort-wasm-simd-threaded.${v}-Q1.mjs`, 'x'));
    assert.ok(starts(m, '(b)'), `${v} glue`);
  }
});

test('(c) first paint, a public page and the service worker must not name them', () => {
  for (const [file, body] of [
    ['app/app-1.js', 'import "./dep-2.js";"silero_vad-Zz9.onnx"'],
    ['app/dep-2.js', 'export const a="ort-wasm-simd-threaded"'],
    ['entrance/landing-1.js', 'import("onnxruntime-web")'],
    ['sw.js', 'precache("/app/ort-wasm-simd-threaded-AbC.wasm")'],
    ['index.html', '<link rel="preload" href="/app/silero_vad-Zz9.onnx">'],
  ]) {
    const r = run((d) => put(d, file, body));
    assert.ok(
      r.some((p) => p.startsWith('(c)') && p.includes(file)),
      file,
    );
  }
});

test('(c) the lazy chunk may name them', () => {
  assert.deepEqual(
    run((d) => put(d, 'app/lazy-vad-3.js', 'ort-wasm silero_vad onnxruntime')),
    [],
  );
});

test('(d) a CDN URL for the runtime in any JavaScript fails', () => {
  for (const url of [
    'https://cdn.jsdelivr.net/npm/onnxruntime-web/dist/',
    'https://unpkg.com/onnxruntime-web',
    'https://onnxruntime.ai/x',
  ]) {
    const r = run((d) => put(d, 'app/lazy-vad-3.js', `const u="${url}"`));
    assert.ok(starts(r, '(d)'), url);
  }
});
