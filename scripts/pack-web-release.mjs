#!/usr/bin/env node
// SEC-AUD-211 (SEC-AUD-196's proof, D-668): the one script that packs the web build for a release.
//
//   node scripts/pack-web-release.mjs <dist dir> <out file>
//
// `.github/workflows/release.yml` (the `web` job) and `release-dry-run.yml` (the `web-bundle` job) both call it, so the
// dry run exercises the path the release takes: only the file name and the publishing differ.
//
// It refuses (exit 1, one line on stderr per reason, nothing written) when
//   - `VITE_SR_E2E` is set (an e2e build compiles in test-only paths, D-570);
//   - the dist holds the e2e voice opt-out marker `sr.e2e.noVoiceE2ee` anywhere;
//   - the service worker was not finished by the build (`__SR_BUILD__` left in `sw.js`, or no `sr-static-` cache name);
//   - one of `app.html`, `app/csp.txt`, `sw.js`, `manifest.webmanifest`, `offline.html` is missing.
// Then it packs the directory as `web/` at the top level of a gzip tar (deploy/INSTALL.md unpacks it with
// `tar -xzf <file> -C <the parent of SR_WEB_DIR>`), lists the archive and fails unless `web/app.html` and
// `web/app/csp.txt` are in it. The system `tar` is used, as the workflows did.

import { execFileSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const REQUIRED_FILES = [
  'app.html',
  'app/csp.txt',
  'sw.js',
  'manifest.webmanifest',
  'offline.html',
];
export const E2E_MARKER = 'sr.e2e.noVoiceE2ee';
export const UNSTAMPED = '__SR_BUILD__';
export const STAMPED = 'sr-static-';
export const MUST_LIST = ['web/app.html', 'web/app/csp.txt'];

/** Every file under `dir`, as paths relative to it with forward slashes. */
function walk(dir, base = dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, base, out);
    else out.push(p.slice(base.length + 1).replaceAll('\\', '/'));
  }
  return out;
}

/** The reasons a dist must not be packed; an empty list means it may be. Reads, never writes. */
export function checkDist(dist, env = process.env) {
  const problems = [];
  if (env.VITE_SR_E2E) {
    problems.push('VITE_SR_E2E is set; a release web build must not set it');
  }
  if (!existsSync(dist) || !statSync(dist).isDirectory()) {
    return [...problems, `${dist} is not a directory (build the web app first)`];
  }
  const files = walk(dist);
  const marked = files.filter((f) => readFileSync(join(dist, f)).includes(E2E_MARKER));
  if (marked.length > 0) {
    problems.push(`the e2e voice opt-out is in the release bundle (${marked.join(', ')})`);
  }
  for (const f of REQUIRED_FILES) {
    if (!files.includes(f)) problems.push(`${f} is missing from ${dist}`);
  }
  if (files.includes('sw.js')) {
    const sw = readFileSync(join(dist, 'sw.js'), 'utf8');
    if (sw.includes(UNSTAMPED)) {
      problems.push(`sw.js still holds ${UNSTAMPED}: the build did not finish the service worker`);
    } else if (!sw.includes(STAMPED)) {
      problems.push(
        `sw.js has no ${STAMPED} cache name: the build did not finish the service worker`,
      );
    }
  }
  return problems;
}

/**
 * Packs `dist` as `web/` into the gzip tar `out`. Throws an Error whose message holds every reason when the dist is
 * refused, or when the finished archive does not list `web/app.html` and `web/app/csp.txt`.
 */
export function packWebRelease(dist, out, env = process.env) {
  const problems = checkDist(dist, env);
  if (problems.length > 0) throw new Error(problems.join('\n'));
  const target = resolve(out);
  const outDir = dirname(target);
  mkdirSync(outDir, { recursive: true });
  // `tar` runs inside the output folder with the archive's bare name: a drive letter in the archive argument is read
  // as a remote host by GNU tar on Windows.
  const stage = join(outDir, `.pack-web-${process.pid}`);
  rmSync(stage, { recursive: true, force: true });
  try {
    mkdirSync(stage, { recursive: true });
    cpSync(resolve(dist), join(stage, 'web'), { recursive: true });
    execFileSync('tar', ['-czf', basename(target), '-C', stage.replace(/\\/g, '/'), 'web'], {
      cwd: outDir,
    });
    const listing = execFileSync('tar', ['-tzf', basename(target)], {
      cwd: outDir,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    })
      .split(/\r?\n/)
      .map((l) => l.trim().replace(/^\.\//, ''));
    const missing = MUST_LIST.filter((f) => !listing.includes(f));
    if (missing.length > 0) {
      rmSync(target, { force: true });
      throw new Error(`the archive does not list ${missing.join(' and ')}`);
    }
    return { out: target, entries: listing.length };
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [dist, out] = process.argv.slice(2);
  if (!dist || !out || process.argv.length > 4) {
    console.error('usage: node scripts/pack-web-release.mjs <dist dir> <out file>');
    process.exit(2);
  }
  try {
    const r = packWebRelease(dist, out);
    console.log(`packed ${dist} as web/ into ${r.out} (${r.entries} entries)`);
  } catch (e) {
    console.error(`pack-web-release: ${e.message}`);
    process.exit(1);
  }
}
