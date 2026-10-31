#!/usr/bin/env node
// Build the built-in Activities (wave 36: ACT-WB-001, ACT-CT-001, ACT-TT-001, ACT-LND-001) into the zips the
// developer portal takes. One shape for all four: each folder under packages/starter-packs/activities is a
// workspace package whose `build` writes the bundle root to its dist/ (index.html and the bundle manifest
// sideroom.json, which declares the `datachannel` capability); this script runs those builds, checks the
// result, and writes packages/starter-packs/activities/dist/<name>.zip (stored entries, the shape
// crates/sr-activities/src/bundle.rs accepts).
//
//   node scripts/build-activities.mjs                    build and zip all four
//   node scripts/build-activities.mjs whiteboard holdem  only these
//   node scripts/build-activities.mjs --no-build         zip what is already in each dist/
//
// How a built-in Activity reaches the shelf (P36-ACT-4): the server's developer uploads the zip as an Activity
// bundle in the developer portal, staff approve it in the review queue like any other bundle (the `datachannel`
// capability is shown with the bundle; no extra approval), and members start it from a voice call. e2e uses
// the same zips (e2e/support/activityBundle.ts `builtInActivityBundle`).

import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32 } from 'node:zlib';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const activitiesDir = join(root, 'packages', 'starter-packs', 'activities');
const outDir = join(activitiesDir, 'dist');

/** How each package builds its dist/ (the same command as its `build` script, without pnpm). */
const vite = (dir) => [join(dir, 'node_modules', 'vite', 'bin', 'vite.js'), 'build'];
export const ACTIVITIES = {
  whiteboard: (dir) => vite(dir),
  holdem: (dir) => vite(dir),
  tabletop: (dir) => [join(dir, 'build.mjs'), join(dir, 'dist')],
  landio: (dir) => [join(dir, 'tools', 'bundle.mjs')],
};

/** Every file under `dir`, keyed by its path relative to `dir` with forward slashes, sorted. */
export function readTree(dir) {
  const files = {};
  const walk = (d) => {
    for (const name of readdirSync(d).sort()) {
      const p = join(d, name);
      if (statSync(p).isDirectory()) walk(p);
      else files[relative(dir, p).split('\\').join('/')] = readFileSync(p);
    }
  };
  walk(dir);
  return files;
}

/** What is wrong with a built bundle root (empty when it is fine). */
export function checkTree(files) {
  const problems = [];
  if (!files['index.html']) problems.push('no index.html at the bundle root');
  const manifest = files['sideroom.json'];
  if (!manifest) problems.push('no sideroom.json at the bundle root');
  else {
    if (manifest.length > 8192) problems.push('sideroom.json is larger than 8 KiB');
    try {
      const m = JSON.parse(manifest.toString('utf8'));
      if (!Array.isArray(m.capabilities) || !m.capabilities.includes('datachannel'))
        problems.push('sideroom.json does not declare the datachannel capability');
    } catch {
      problems.push('sideroom.json is not valid JSON');
    }
  }
  if (Object.keys(files).some((f) => f.endsWith('.zip'))) problems.push('a zip inside the bundle');
  return problems;
}

/** A zip of stored entries (the same writer as e2e/support/activityBundle.ts). */
export function zip(entries) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  const names = Object.keys(entries);
  for (const name of names) {
    const data = Buffer.from(entries[name]);
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
  end.writeUInt16LE(names.length, 8);
  end.writeUInt16LE(names.length, 10);
  end.writeUInt32LE(dir.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, dir, end]);
}

function main(argv) {
  const build = !argv.includes('--no-build');
  const names = argv.filter((a) => !a.startsWith('--'));
  const pick = names.length ? names : Object.keys(ACTIVITIES);
  let failed = false;
  mkdirSync(outDir, { recursive: true });
  for (const name of pick) {
    const cmd = ACTIVITIES[name];
    if (!cmd) {
      console.error(`unknown activity: ${name} (known: ${Object.keys(ACTIVITIES).join(', ')})`);
      failed = true;
      continue;
    }
    const dir = join(activitiesDir, name);
    if (build) execFileSync(process.execPath, cmd(dir), { cwd: dir, stdio: ['ignore', 'ignore', 'inherit'] });
    const files = readTree(join(dir, 'dist'));
    const problems = checkTree(files);
    if (problems.length) {
      for (const p of problems) console.error(`${name}: ${p}`);
      failed = true;
      continue;
    }
    const out = join(outDir, `${name}.zip`);
    const body = zip(files);
    writeFileSync(out, body);
    console.log(`${name}: ${Object.keys(files).length} files, ${body.length} bytes -> ${relative(root, out)}`);
  }
  if (failed) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
