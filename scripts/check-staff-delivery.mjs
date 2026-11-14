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
//   e. a file under staff/ has a name that is not a bare hash, or is not .js or .css,
//   f. a staff message's text (a value of 12 characters or more in messages/en-staff that no member fragment also
//      holds) is in a file outside /app/staff/ or in app.html (the key is printed, never the text; SEC-AUD-187, D-663),
//   g. dist/app/staff/ holds no catalog: none of the staff messages is inside it (the staff catalog is missing, or the
//      fragments moved and the build did not follow),
//   h. a file under dist/app/ outside staff/ is the build report `staff-chunks.json` (SEC-AUD-271, D-652): the server
//      sends every file outside staff/ to any signed-in member, and the report names every staff chunk and module.
// It also prints a REPORT (never a failure): modules outside src/admin/ whose path looks like staff code and that sit in
// member chunks (the wave 66 list), read from the build report apps/web/staff-chunks.json (beside dist/, not in it).
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { staticImports } from './lib/bundle.mjs';
import { readEnglishCatalog } from './pseudo-locale.mjs';

/**
 * String literals that exist only in `src/admin/AdminApp.tsx` (its section ids). They survive minification and are
 * not catalog keys (the staff catalog holds their texts, not these ids; D-663). If AdminApp renames them, update this list:
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

const here = dirname(fileURLToPath(import.meta.url));
const MESSAGES = join(here, '..', 'apps', 'web', 'src', 'i18n', 'messages');
const SRC = join(here, '..', 'apps', 'web', 'src');
const CHANGELOG = join(here, '..', 'CHANGELOG.md');
const MIN_TEXT = 12;

/** The longest run of plain ASCII (no quote, backslash, newline, backtick) in a message: what a minifier leaves as is. */
function needleOf(text) {
  let best = '';
  for (const part of text.split(/[^ -~]|['"`\\]/)) if (part.length > best.length) best = part;
  best = best.trim();
  return best.length >= MIN_TEXT ? best : null;
}

/** Text a member can legitimately be sent: code outside src/admin/, and the changelog the What's new page shows. */
function memberProse(srcRoot, changelog) {
  const parts = [];
  if (changelog && existsSync(changelog)) parts.push(readFileSync(changelog, 'utf8'));
  const visit = (dir, rel) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const r = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) {
        if (e.name === '__tests__' || e.name === 'node_modules') continue;
        if (r === 'admin' || r === 'i18n/messages' || r === 'test') continue;
        visit(join(dir, e.name), r);
      } else if (/.(ts|tsx|md)$/.test(e.name) && !/.(test|spec)./.test(e.name))
        parts.push(readFileSync(join(dir, e.name), 'utf8'));
    }
  };
  if (existsSync(srcRoot)) visit(srcRoot, '');
  return parts.join('\n');
}

/**
 * The staff catalog as the check needs it: `{ needles: [[key, needle]] }`, one needle per staff message of 12 characters
 * or more that nothing a member is sent could also hold: not a text a member fragment holds, and not a text that
 * member code or the changelog (the What's new page, the developer docs) holds as prose. `dir` is
 * apps/web/src/i18n/messages.
 */
export function staffCatalogNeedles(
  dir = MESSAGES,
  prose = dir === MESSAGES ? { src: SRC, changelog: CHANGELOG } : null,
) {
  const staff = readEnglishCatalog(join(dir, 'en-staff'), false);
  const member = readEnglishCatalog(join(dir, 'en'), false);
  const blob = [
    Object.values(member).join('\n'),
    prose ? memberProse(prose.src, prose.changelog) : '',
  ].join('\n');
  const needles = [];
  for (const [key, text] of Object.entries(staff)) {
    const needle = needleOf(text);
    if (needle && !blob.includes(needle)) needles.push([key, needle]);
  }
  return { needles };
}

/**
 * Returns `{ failures: string[], report: string[], staffFiles: string[] }`. `catalog` (from `staffCatalogNeedles`)
 * switches on rules f and g; the unit cases of a..e pass none.
 */
export function checkStaffDelivery(
  dist,
  markers = STAFF_MARKERS,
  catalog = null,
  manifestFile = join(dist, '..', 'staff-chunks.json'),
) {
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

  // f, g (SEC-AUD-187, D-663): the staff catalog is inside staff/ and nowhere else
  if (catalog) {
    const text = (f) => readFileSync(join(app, f), 'utf8');
    const outside = others
      .filter((x) => /\.(js|css|html|json|webmanifest|map)$/.test(x))
      .map((f) => [`app/${f}`, text(f)]);
    if (shellText) outside.push(['app.html', shellText]);
    const inside = staffFiles.map(text);
    let found = 0;
    for (const [key, needle] of catalog.needles) {
      const where = outside.find(([, code]) => code.includes(needle));
      if (where)
        failures.push(`f: the staff message ${key} is in ${where[0]}, outside /app/staff/`);
      if (inside.some((code) => code.includes(needle))) found++;
    }
    if (catalog.needles.length > 0 && found === 0)
      failures.push(
        'g: dist/app/staff/ holds no staff catalog: none of the staff messages is inside it',
      );
  }

  // h
  for (const f of others.filter((x) => posix.basename(x).startsWith('staff-chunks')))
    failures.push(
      `h: app/${f} is a file every signed-in member can fetch and it lists the staff chunks: the build report belongs beside dist/, not in it`,
    );

  // report
  if (existsSync(manifestFile)) {
    const m = JSON.parse(readFileSync(manifestFile, 'utf8'));
    const looks = m.memberModulesStaffLooking ?? [];
    report.push(
      `report: ${looks.length} module(s) outside src/admin/ look like staff code and sit in member chunks (wave 66):`,
      ...looks.map((x) => `  ${x}`),
      `report: ${(m.memberModulesUnderAdmin ?? []).length} module(s) under src/admin/ are shared with member code and stay in member chunks`,
    );
  } else report.push(`report: the build report ${manifestFile} is missing (the build writes it)`);
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
  const { failures, report, staffFiles } = checkStaffDelivery(
    dist,
    STAFF_MARKERS,
    staffCatalogNeedles(),
  );
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
