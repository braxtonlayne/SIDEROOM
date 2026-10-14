#!/usr/bin/env node
// Docs link checker (XC-CI-003): every relative Markdown link must point at a file that exists, and every
// #fragment must match a heading in the target. External links are not fetched (CI stays offline).
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  'dist',
  '.git',
  'tests',
  'corpus',
  '.state',
  'test-results',
]);

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) {
      if (!SKIP_DIRS.has(name)) walk(p, out);
    } else if (name.endsWith('.md')) out.push(p);
  }
  return out;
}

export function slug(heading) {
  return heading
    .toLowerCase()
    .replace(/`/g, '')
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/\s/g, '-');
}

const anchorCache = new Map();
function anchors(file) {
  if (!anchorCache.has(file)) {
    const set = new Set();
    let fence = false;
    for (const line of readFileSync(file, 'utf8').split(/\r?\n/)) {
      if (/^\s*```/.test(line)) fence = !fence;
      const h = !fence && /^#{1,6}\s+(.*?)\s*#*\s*$/.exec(line);
      if (h) set.add(slug(h[1]));
    }
    anchorCache.set(file, set);
  }
  return anchorCache.get(file);
}

export function linksIn(text) {
  const out = [];
  let fence = false;
  text.split(/\r?\n/).forEach((line, i) => {
    if (/^\s*```/.test(line)) fence = !fence;
    if (fence) return;
    const bare = line.replace(/`[^`]*`/g, '');
    for (const m of bare.matchAll(/\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g))
      out.push({ target: m[1], line: i + 1 });
  });
  return out;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const problems = [];
  for (const file of walk(root)) {
    for (const { target, line } of linksIn(readFileSync(file, 'utf8'))) {
      if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith('//')) continue; // http:, mailto:, ...
      const [pathPart, frag] = target.split('#');
      const dest = pathPart ? resolve(dirname(file), decodeURIComponent(pathPart)) : file;
      const where = `${relative(root, file)}:${line}`;
      if (!existsSync(dest)) {
        problems.push(`${where}: missing file ${target}`);
      } else if (
        frag &&
        dest.endsWith('.md') &&
        !statSync(dest).isDirectory() &&
        !anchors(dest).has(frag.toLowerCase())
      ) {
        problems.push(`${where}: no heading "#${frag}" in ${relative(root, dest)}`);
      }
    }
  }

  if (problems.length) {
    console.error(problems.join('\n'));
    console.error(`\n${problems.length} broken link(s)`);
    process.exit(1);
  }
  console.log('doc links ok');
}
