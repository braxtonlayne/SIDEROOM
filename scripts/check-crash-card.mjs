#!/usr/bin/env node
// OPS-CRASH-001 (wave 45-A): the secret-pattern grep of a built crash card, runnable in CI without a download.
//
// A crash card is built from an allow-list (apps/web/src/crash/allowList.ts, crates/sr-api/src/routes/crash/allow_list.rs).
// This is the independent second check: it greps the BUILT card (or any text) against the repo-wide secret patterns of
// scripts/secret-scan.mjs plus the crash-specific shapes of scripts/crash-secret-patterns.json (bearer tokens, JWTs,
// presigned-URL queries, addresses, e-mails, URLs, long hex runs). It NEVER prints a matched value: a hit is reported as
// the pattern name only.
//
// Usage:
//   node scripts/check-crash-card.mjs <file>...      grep each file; exit 1 on any hit
//   node scripts/check-crash-card.mjs --self-test    the hostile payload of e2e/fixtures/crash-hostile.json must trip the
//                                                    patterns and its expected card must pass them (proves the grep bites)
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PATTERNS as REPO_PATTERNS } from './secret-scan.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

const extra = JSON.parse(
  readFileSync(join(ROOT, 'scripts', 'crash-secret-patterns.json'), 'utf8'),
).patterns;

/** Every pattern applied to a crash card: the repo-wide list, then the crash shapes. */
export const CRASH_PATTERNS = [
  ...REPO_PATTERNS.map((p) => ({ name: p.name, re: p.re, reject: p.reject })),
  ...extra.map((p) => ({ name: p.name, re: new RegExp(p.re, p.flags), reject: undefined })),
];

/** Names of the patterns that match `text` (each once, never the matched text). */
export function findLeaks(text) {
  const hits = new Set();
  for (const line of text.split(/\r?\n/)) {
    for (const p of CRASH_PATTERNS) {
      const m = p.re.exec(line);
      if (!m) continue;
      if (p.reject?.(m)) continue;
      hits.add(p.name);
    }
  }
  return [...hits];
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
function main() {
  const argv = process.argv.slice(2);
  if (argv.includes('--self-test')) {
    const fx = JSON.parse(
      readFileSync(join(ROOT, 'e2e', 'fixtures', 'crash-hostile.json'), 'utf8'),
    );
    const hostile = findLeaks(JSON.stringify(fx.payload, null, 2));
    const built = findLeaks(JSON.stringify(fx.expect, null, 2));
    console.log(`hostile payload trips ${hostile.length} patterns: ${hostile.join(', ')}`);
    console.log(`expected card trips ${built.length} patterns`);
    if (hostile.length < 6 || built.length) process.exit(1);
    return;
  }
  const files = argv.filter((a) => !a.startsWith('--'));
  if (!files.length) {
    console.error('usage: check-crash-card.mjs <file>... | --self-test');
    process.exit(2);
  }
  let bad = 0;
  for (const f of files) {
    const hits = findLeaks(readFileSync(f, 'utf8'));
    if (hits.length) {
      bad += 1;
      console.error(`${f}: secret patterns found: ${hits.join(', ')}`);
    } else {
      console.log(`${f}: clean`);
    }
  }
  process.exit(bad ? 1 : 0);
}
