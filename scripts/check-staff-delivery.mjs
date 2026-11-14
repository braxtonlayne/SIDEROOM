#!/usr/bin/env node
// Staff delivery check (SEC-AUD-170, D-651; docs/dev/entrance-isolation.md "Staff code").
//
//   node scripts/check-staff-delivery.mjs [distDir] [--quiet]
//
// The app build (apps/web/vite/staffChunks.ts) sorts every chunk that only a staff entry reaches into dist/app/staff/,
// named by its hash alone; sr-server serves that directory only to appointed staff. Run after a build, beside the
// bundle budget. Fails when:
//   a. dist/app/staff/ is missing or empty,
//   b. dist/app.html references anything under /app/staff/ (script, modulepreload, stylesheet, anything),
//   c. a file outside staff/ has a static import of a staff/ file,
//   d. a file outside staff/ holds a marker of the staff entry (a string literal that exists only in AdminApp.tsx),
//   e. a file under staff/ has a name that is not a bare hash, or is not .js or .css.
// It also prints a REPORT (never a failure): modules outside src/admin/ whose path looks like staff code and that sit in
// member chunks (the wave 66 list), read from dist/app/staff-chunks.json.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { staticImports } from './lib/bundle.mjs';

/**
 * String literals that exist only in `src/admin/AdminApp.tsx` (its section ids). They survive minification and are
 * not catalog keys (the catalog is a member chunk until wave 66, D-658). If AdminApp renames them, update this list:
 * the check fails when none of the markers is found inside staff/.
 */
export const STAFF_MARKERS = ['donation-entitlements', 'cinema-transcoding', 'invite-graph'];

const HASH_NAME = /^[A-Za-z0-9_-]{8}\.(js|css)$/;

function walk(dir, base = '') {
  const out = [];
  if (!existsSync(dir)) return out;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const rel = base ? `${base}/${e.name}` : e.name;
    if (e.isDirectory()) out.push(...walk(join(dir, e.name), rel));
    else out.push(rel);
  }
  return out;
}

/** Returns `{ failures: string[], report: string[], staffFiles: string[] }`. */
export function checkStaffDelivery(dist, markers = STAFF_MARKERS) {
  const failures = [];
  const report = [];
  const app = join(dist, 'app');
  const all = walk(app);
  const staffFiles = all.filter((f) => f.startsWith('staff/'));
  const others = all.filter((f) => !f.startsWith('staff/'));

  // a
  if (staffFiles.length === 0) failures.push('a: dist/app/staff/ is missing or empty');

  // b
  const shell = join(dist, 'app.html');
  if (!existsSync(shell)) failures.push('b: dist/app.html is missing');
  else if (/\/app\/staff\//.test(readFileSync(shell, 'utf8')))
    failures.push('b: dist/app.html references a file under /app/staff/');

  // c
  for (const f of others.filter((x) => x.endsWith('.js'))) {
    const rel = `app/${f}`;
    for (const imp of staticImports(rel, readFileSync(join(dist, rel), 'utf8'))) {
      if (imp.startsWith('app/staff/'))
        failures.push(
          `c: ${rel} statically imports ${imp}: a member chunk must not load staff code`,
        );
    }
  }

  // d
  for (const f of others.filter((x) => /\.(js|css|html|json|webmanifest)$/.test(x))) {
    const code = readFileSync(join(app, f), 'utf8');
    for (const m of markers)
      if (code.includes(m)) failures.push(`d: app/${f} holds the staff marker "${m}"`);
  }
  const shellText = existsSync(shell) ? readFileSync(shell, 'utf8') : '';
  for (const m of markers)
    if (shellText.includes(m)) failures.push(`d: app.html holds the staff marker "${m}"`);
  if (staffFiles.length) {
    const inStaff = markers.filter((m) =>
      staffFiles.some((f) => readFileSync(join(app, f), 'utf8').includes(m)),
    );
    if (inStaff.length === 0)
      failures.push(
        'd: no staff marker is inside staff/: the markers in check-staff-delivery.mjs are stale',
      );
  }

  // e
  for (const f of staffFiles) {
    if (posix.dirname(f) !== 'staff' || !HASH_NAME.test(posix.basename(f)))
      failures.push(`e: app/${f} is not named <hash>.js or <hash>.css`);
  }

  // report
  const manifestFile = join(app, 'staff-chunks.json');
  if (existsSync(manifestFile)) {
    const m = JSON.parse(readFileSync(manifestFile, 'utf8'));
    const looks = m.memberModulesStaffLooking ?? [];
    report.push(
      `report: ${looks.length} module(s) outside src/admin/ look like staff code and sit in member chunks (wave 66):`,
      ...looks.map((x) => `  ${x}`),
      `report: ${(m.memberModulesUnderAdmin ?? []).length} module(s) under src/admin/ are shared with member code and stay in member chunks`,
    );
  } else report.push('report: dist/app/staff-chunks.json is missing');
  return { failures, report, staffFiles };
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const here = dirname(fileURLToPath(import.meta.url));
  const args = process.argv.slice(2);
  const dist = resolve(
    args.find((a) => !a.startsWith('--')) ?? join(here, '..', 'apps', 'web', 'dist'),
  );
  if (!existsSync(join(dist, 'app'))) {
    console.error(`No build at ${dist}. Run: pnpm --filter @sideroom/web build`);
    process.exit(2);
  }
  const { failures, report, staffFiles } = checkStaffDelivery(dist);
  if (!args.includes('--quiet')) {
    const bytes = staffFiles.reduce((n, f) => n + readFileSync(join(dist, 'app', f)).length, 0);
    console.log(`staff files: ${staffFiles.length}, ${(bytes / 1024).toFixed(1)} KB raw`);
    console.log(report.join('\n'));
  }
  if (failures.length) {
    console.error(`\nStaff delivery FAILED:\n${failures.map((f) => `  ${f}`).join('\n')}`);
    process.exit(1);
  }
  console.log(
    'ok   staff delivery: staff chunks are under /app/staff/ and nothing outside loads or holds them',
  );
}
