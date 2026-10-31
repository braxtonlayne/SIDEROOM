// Build the Tabletop Activity bundle: `node packages/starter-packs/activities/tabletop/build.mjs [outdir]`.
// Copies index.html, style.css and src/ (not tests/), transpiles the Embedded App SDK from
// packages/embedded-app-sdk/src into src/sdk/ as browser modules, and writes tabletop.zip (stored entries, the
// shape crates/sr-activities/src/bundle.rs accepts) next to the folder. No dependencies beyond the repo's own
// `typescript`.

import { createRequire } from 'node:module';
import { crc32 } from 'node:zlib';
import { mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..', '..', '..');
const out = resolve(process.argv[2] ?? join(here, 'dist'));

const sdkDir = join(repo, 'packages', 'embedded-app-sdk');
const ts = createRequire(join(sdkDir, 'package.json'))('typescript');

/** @type {Record<string, string | Buffer>} */
const files = {};
files['index.html'] = readFileSync(join(here, 'index.html'));
files['style.css'] = readFileSync(join(here, 'style.css'));
files['sideroom.json'] = readFileSync(join(here, 'sideroom.json'));
for (const name of readdirSync(join(here, 'src'))) {
  if (name.endsWith('.js')) files[`src/${name}`] = readFileSync(join(here, 'src', name));
}
// The SDK: index, sdk, protocol and data (the data channel, ACT-CH-001).
for (const name of ['index', 'sdk', 'protocol', 'data']) {
  const source = join(sdkDir, 'src', `${name}.ts`);
  try {
    statSync(source);
  } catch {
    continue;
  }
  const js = ts.transpileModule(readFileSync(source, 'utf8'), {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ES2020 },
  }).outputText;
  files[`src/sdk/${name}.js`] = js.replace(/from '\.\/(\w+)'/g, "from './$1.js'");
}

rmSync(out, { recursive: true, force: true });
for (const [name, body] of Object.entries(files)) {
  const target = join(out, name);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, body);
}

/** A zip of stored entries (the same writer as e2e/support/activityBundle.ts). */
function zip(entries) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const [name, body] of Object.entries(entries)) {
    const data = Buffer.from(body);
    const nameBuf = Buffer.from(name, 'utf8');
    const crc = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt16LE(0x21, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    locals.push(local, nameBuf, data);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(0x031e, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(0x21, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBuf.length, 28);
    central.writeUInt32LE((0o100644 << 16) >>> 0, 38);
    central.writeUInt32LE(offset, 42);
    centrals.push(central, nameBuf);
    offset += 30 + nameBuf.length + data.length;
  }
  const dir = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(entries).length, 8);
  end.writeUInt16LE(Object.keys(entries).length, 10);
  end.writeUInt32LE(dir.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, dir, end]);
}

const zipPath = `${out}.zip`;
writeFileSync(zipPath, zip(files));
console.log(`${Object.keys(files).length} files -> ${relative(process.cwd(), out)} and ${relative(process.cwd(), zipPath)}`);
