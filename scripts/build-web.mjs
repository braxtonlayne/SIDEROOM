#!/usr/bin/env node
// SEC-ENT-001 (D-385): builds the web client as TWO builds into one `apps/web/dist`.
//
//   1. the public build  (vite.public.config.ts): index.html, auth.html, invite.html, /entrance/*, the public files
//   2. the app build     (vite.config.ts):        app.html, /app/*, app/csp.txt, app/routes.json
//   3. dist/surfaces.json (a copy of apps/web/public-surfaces.json), the legal pages, the split notices
//
// Layout (docs/dev/entrance-isolation.md section 2; docs/dev/handoff/feat-p30b-ent-build.md):
// the line between public and private is a directory, never a chunking decision inside one Rollup run.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const web = join(root, 'apps/web');
const dist = join(web, 'dist');
const vite = createRequire(join(web, 'package.json')).resolve('vite/bin/vite.js');

function run(label, args, cwd = web) {
  console.log(`\n[build-web] ${label}`);
  const res = spawnSync(process.execPath, args, { cwd, stdio: 'inherit', env: process.env });
  if (res.status !== 0) {
    console.error(`[build-web] ${label} failed (exit ${res.status ?? res.signal})`);
    process.exit(res.status ?? 1);
  }
}

rmSync(dist, { recursive: true, force: true });
run('public build (vite.public.config.ts)', [vite, 'build', '--config', 'vite.public.config.ts']);
run('app build (vite.config.ts)', [vite, 'build']);
copyFileSync(join(web, 'public-surfaces.json'), join(dist, 'surfaces.json'));
for (const step of ['render-legal', 'split-notices']) {
  const script = join(root, 'scripts', `${step}.mjs`);
  if (existsSync(script)) run(step, [script], root);
}
console.log('\n[build-web] done: dist/{index,auth,invite,app}.html, dist/entrance/, dist/app/, dist/surfaces.json');
