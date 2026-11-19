#!/usr/bin/env node
// VOX-CLI-002 (D-1049): the Silero VAD model and its runtime are pinned, self-hosted and lazy.
// Usage (after a web build): node scripts/check-vad-assets.mjs [distDir]
// Fails when
//   (a) the vendored model's SHA-256 is not the one in apps/web/vendor/silero-vad/SHA256SUMS;
//   (b) the build lacks the model or the plain one-thread wasm, the built model differs from the vendored one,
//       or any other runtime variant (jsep, jspi, asyncify, a second wasm or model) was emitted;
//   (c) first paint (app.html, its static closure), a public page or surface, the service worker or any file at the
//       top of dist names the runtime, the wasm or the model;
//   (d) any built JavaScript holds a CDN URL for the runtime (cdn.jsdelivr.net, unpkg.com, onnxruntime plus a host).
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { staticImports } from './lib/bundle.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const VENDOR = join(here, '..', 'apps', 'web', 'vendor', 'silero-vad');

const sha = (buf) => createHash('sha256').update(buf).digest('hex');
const MARKERS = ['ort-wasm', 'silero_vad', 'onnxruntime'];
const CDN = /cdn\.jsdelivr\.net|unpkg\.com|onnxruntime[\w.-]*\.(?:com|net|org|io|ai|dev|app)\b/i;

/** Every file under dir, as posix paths relative to it. */
function walk(dir, base = '') {
  const out = [];
  for (const name of readdirSync(join(dir, base))) {
    const rel = base ? `${base}/${name}` : name;
    if (statSync(join(dir, rel)).isDirectory()) out.push(...walk(dir, rel));
    else out.push(rel);
  }
  return out;
}

const text = (dist, f) => readFileSync(join(dist, f), 'utf8');

/** First-paint closure of a shell page: the scripts it names and their static imports. */
function closure(dist, shell) {
  const seen = new Set();
  if (!existsSync(join(dist, shell))) return seen;
  const roots = [...text(dist, shell).matchAll(/(?:src|href)="\/([^"]+\.m?js)"/g)].map((m) => m[1]);
  const stack = [...roots];
  while (stack.length) {
    const f = stack.pop();
    if (seen.has(f) || !existsSync(join(dist, f))) continue;
    seen.add(f);
    stack.push(...staticImports(f, text(dist, f)));
  }
  return seen;
}

/** Returns the list of problems (empty when the build is right). */
export function problems(distDir, vendorDir = VENDOR) {
  const dist = resolve(distDir);
  const bad = [];

  // (a) the vendored model against SHA256SUMS
  const sums = new Map();
  try {
    for (const line of readFileSync(join(vendorDir, 'SHA256SUMS'), 'utf8').split(/\r?\n/)) {
      const m = /^([0-9a-f]{64}) [ *](.+)$/.exec(line.trim());
      if (m) sums.set(m[2], m[1]);
    }
  } catch {
    bad.push('(a) SHA256SUMS is missing from the vendor folder');
  }
  let vendored = null;
  try {
    vendored = readFileSync(join(vendorDir, 'silero_vad.onnx'));
  } catch {
    bad.push('(a) the vendored silero_vad.onnx is missing');
  }
  if (vendored && sums.get('silero_vad.onnx') !== sha(vendored)) {
    bad.push(
      `(a) vendored silero_vad.onnx is ${sha(vendored)}, SHA256SUMS says ${sums.get('silero_vad.onnx') ?? 'nothing'}`,
    );
  }

  if (!existsSync(dist)) return [...bad, `no build at ${dist}`];
  const files = walk(dist);

  // (b) the built model and wasm
  const models = files.filter((f) => /\.onnx$/.test(f));
  const builtModels = models.filter((f) => /(^|\/)silero_vad[^/]*\.onnx$/.test(f));
  const wasm = files.filter((f) => /\.wasm$/.test(f));
  const plainWasm = wasm.filter((f) => /(^|\/)ort-wasm-simd-threaded-[^/.]+\.wasm$/.test(f));
  if (builtModels.length !== 1) {
    bad.push(`(b) expected one built silero_vad*.onnx, found ${builtModels.length}`);
  } else if (vendored && sha(readFileSync(join(dist, builtModels[0]))) !== sha(vendored)) {
    bad.push(`(b) ${builtModels[0]} differs from the vendored model`);
  }
  if (models.length !== builtModels.length) {
    const extra = models.filter((f) => !builtModels.includes(f));
    bad.push(`(b) an unexpected .onnx file is in the build: ${extra.join(', ')}`);
  }
  if (plainWasm.length !== 1) {
    bad.push(
      `(b) expected one built ort-wasm-simd-threaded-<hash>.wasm, found ${plainWasm.length}`,
    );
  } else if (statSync(join(dist, plainWasm[0])).size < 1_000_000) {
    bad.push(`(b) ${plainWasm[0]} is too small to be the runtime`);
  }
  const variants = files.filter(
    (f) =>
      /ort-.*(jsep|jspi|asyncify|webgpu|webgl)/i.test(f) ||
      (/(^|\/)ort-/.test(f) && !plainWasm.includes(f)),
  );
  if (variants.length) {
    bad.push(
      `(b) other runtime files were emitted (one-thread CPU wasm only): ${variants.join(', ')}`,
    );
  }

  // (c) nothing at first paint, on a public page or in the service worker names them
  const names = [...builtModels, ...plainWasm].map((f) => posix.basename(f));
  const markers = [...MARKERS, ...names];
  const watched = new Set();
  for (const f of files) {
    if (/^entrance\//.test(f) || !f.includes('/')) watched.add(f);
  }
  for (const s of files.filter((f) => /^[^/]+\.html$/.test(f))) {
    for (const f of closure(dist, s)) watched.add(f);
  }
  for (const f of [...watched].sort()) {
    if (!/\.(m?js|html|json|webmanifest|css|txt|md)$/.test(f)) continue;
    const t = text(dist, f);
    const hit = markers.find((m) => t.includes(m));
    if (hit)
      bad.push(`(c) ${f} names "${hit}": it is first paint, a public file or the service worker`);
  }

  // (d) no CDN for the runtime in any built JavaScript
  for (const f of files) {
    if (!/\.m?js$/.test(f)) continue;
    const m = CDN.exec(text(dist, f));
    if (m) bad.push(`(d) ${f} holds a CDN URL for the runtime ("${m[0]}")`);
  }
  return bad;
}

const main = () => {
  const dist = process.argv[2] ?? join(here, '..', 'apps', 'web', 'dist');
  const bad = problems(dist);
  if (bad.length) {
    console.error(`VAD asset check failed:\n  ${bad.join('\n  ')}`);
    process.exit(1);
  }
  console.log(
    'VAD asset check ok: model pinned, one-thread wasm and model self-hosted, nothing at first paint.',
  );
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
