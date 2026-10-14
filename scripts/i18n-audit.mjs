#!/usr/bin/env node
// Counts probable un-extracted user-visible strings in non-test .tsx files.
// Usage: node scripts/i18n-audit.mjs [apps/web/src] [--json out.json]
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

const args = process.argv.slice(2);
const jsonIdx = args.indexOf('--json');
const jsonOut = jsonIdx >= 0 ? args[jsonIdx + 1] : undefined;
if (jsonIdx >= 0) args.splice(jsonIdx, 2);
const root = args[0] ?? 'apps/web/src';

const SKIP_DIRS = new Set(['__tests__', 'i18n', 'test', 'styles', 'node_modules']);
const SKIP_PATHS = ['routes/entrance'];
const SKIP_FILE = /(^Entrance|^SkipLink|^Door|^icons?\b|Icons?\.tsx$)/i;

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (SKIP_DIRS.has(name)) continue;
      const rel = relative(root, p).split(sep).join('/');
      if (SKIP_PATHS.includes(rel)) continue;
      walk(p, out);
    } else if (name.endsWith('.tsx') && !/\.test\.tsx$/.test(name) && !SKIP_FILE.test(name)) {
      out.push(p);
    }
  }
  return out;
}

const letters = (s) => (s.match(/\p{L}/gu) ?? []).length;
const ATTR =
  /\b(?:aria-label|title|placeholder|alt|label|description|helpText)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;

export function countFile(src) {
  let n = 0;
  // Attribute string literals.
  for (const m of src.matchAll(ATTR)) if (letters(m[1] ?? m[2] ?? '') >= 3) n++;
  // JSX text: text between '>' and '<' outside braces, at least 3 letters.
  const noComments = src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');
  for (const m of noComments.matchAll(/>([^<>{}]*)(?=<)/g)) {
    const text = m[1].replace(/&[a-z]+;/g, ' ');
    if (letters(text) < 3) continue;
    // Skip TypeScript generics / arrows / comparisons that merely look like text.
    if (/[=;()]|=>/.test(text) && !/[A-Za-z]{3,}\s+[A-Za-z]{2,}/.test(text)) continue;
    n++;
  }
  return n;
}

const rows = [];
for (const file of walk(root)) {
  const count = countFile(readFileSync(file, 'utf8'));
  if (count > 0) rows.push({ file: relative(root, file).split(sep).join('/'), count });
}
rows.sort((a, b) => b.count - a.count || a.file.localeCompare(b.file));
const total = rows.reduce((s, r) => s + r.count, 0);

const width = Math.max(4, ...rows.map((r) => r.file.length));
for (const r of rows) console.log(`${String(r.count).padStart(5)}  ${r.file.padEnd(width)}`);
console.log(`TOTAL ${total}`);
if (jsonOut) writeFileSync(jsonOut, JSON.stringify({ total, files: rows }, null, 2) + '\n');
