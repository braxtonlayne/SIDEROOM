#!/usr/bin/env node
// Heuristic audit: English text removed from non-test .tsx files by the working-tree diff should now live
// in the catalog (verbatim, or with ICU {vars} as wildcards). Prints what is not found. Always exits 0.
// Usage: node scripts/i18n-verify-diff.mjs [git diff args...]
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const FRAGMENTS = 'apps/web/src/i18n/messages/en';
const extra = process.argv.slice(2);

const values = [];
for (const f of readdirSync(FRAGMENTS).filter((n) => n.endsWith('.ts'))) {
  const src = readFileSync(join(FRAGMENTS, f), 'utf8');
  const body = src
    .replace(/^\s*export\s+default\s*/m, 'return ')
    .replace(/\}\s*satisfies[^;]*;?\s*$/m, '}');
  try {
    values.push(...Object.values(new Function(body)()));
  } catch {
    // Tolerant fallback: quoted values after `'key':`.
    for (const m of src.matchAll(
      /'[\w.]+':\s*(?:'((?:[^'\x5c]|\x5c.)*)'|"((?:[^"\x5c]|\x5c.)*)")/g,
    )) {
      values.push(m[1] ?? m[2]);
    }
  }
}

// ICU message -> regex: {var} and {n, plural, ...} become wildcards, the rest is literal.
function toRegex(msg) {
  let out = '';
  let depth = 0;
  for (const ch of msg) {
    if (ch === '{') {
      if (depth === 0) out += '[^]*?';
      depth++;
    } else if (ch === '}') depth = Math.max(0, depth - 1);
    else if (depth === 0) out += /[A-Za-z0-9 ]/.test(ch) || ch > '~' ? ch : '\x5c' + ch;
  }
  return new RegExp(out);
}
// Messages that are (nearly) all placeholders would match anything, so only use ones with literal text.
const literalLetters = (v) => letters(v.replace(/{[^{}]*(?:{[^{}]*}[^{}]*)*}/g, ''));
const catalog = values
  .filter((v) => literalLetters(v) >= 3)
  .map((v) => ({ text: v, re: toRegex(v) }));
const catalogText = values.join('\n');

function found(text) {
  if (catalogText.includes(text)) return true;
  return catalog.some((c) => c.re.test(text));
}

function letters(s) {
  return (s.match(/\p{L}/gu) ?? []).length;
}

let diff = '';
try {
  diff = execFileSync('git', ['diff', '-U0', ...extra, '--', 'apps/web/src'], {
    encoding: 'utf8',
    maxBuffer: 1 << 28,
  });
} catch (e) {
  console.error('git diff failed:', e.message);
}

const removed = []; // { file, text }
let file = '';
for (const line of diff.split('\n')) {
  if (line.startsWith('+++ ')) file = line.slice(6);
  else if (line.startsWith('-') && !line.startsWith('--- ')) {
    if (!file.endsWith('.tsx') || /\.test\.tsx$|__tests__|\/i18n\//.test(file)) continue;
    const l = line.slice(1);
    const texts = [];
    for (const m of l.matchAll(/"([^"]*)"|'([^']*)'|`([^`]*)`/g)) texts.push(m[1] ?? m[2] ?? m[3]);
    // JSX text on the line: between '>' and '<', or a whole line of plain text.
    for (const m of l.matchAll(/>([^<>{}]+)(?=<|$)/g)) texts.push(m[1]);
    if (!/[<>{}=;()]/.test(l) && /^\s+\S/.test(l)) texts.push(l);
    for (const raw of texts) {
      const text = raw.trim().replace(/\s+/g, ' ');
      if (letters(text) < 3) continue;
      // Skip things that are clearly code: class lists, paths, identifiers, imports.
      if (/^[\w./:@#-]+$/.test(text) && !/\s/.test(text) && !/^[A-Z][a-z]+$/.test(text)) continue;
      if (/^(import|export|from)\b/.test(l.trim())) continue;
      removed.push({ file, text });
    }
  }
}

const missing = [];
const seen = new Set();
for (const r of removed) {
  const k = `${r.file}\u0000${r.text}`;
  if (seen.has(k)) continue;
  seen.add(k);
  const flat = catalogText.replace(/\s+/g, ' ');
  if (!found(r.text) && !flat.includes(r.text)) missing.push(r);
}
for (const m of missing) console.log(`${m.file}: ${JSON.stringify(m.text)}`);
console.log(`MISSING ${missing.length}`);
process.exit(0);
