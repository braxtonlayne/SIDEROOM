#!/usr/bin/env node
// Counts probable un-extracted user-visible strings in non-test .tsx files.
// Usage: node scripts/i18n-audit.mjs [apps/web/src] [--json out.json] [--ts]
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

const args = process.argv.slice(2);
const jsonIdx = args.indexOf('--json');
const jsonOut = jsonIdx >= 0 ? args[jsonIdx + 1] : undefined;
if (jsonIdx >= 0) args.splice(jsonIdx, 2);
// --ts also scans plain .ts modules (label tables, formatters) with a stricter heuristic: a quoted or template
// literal of two or more words, starting with a capital, on a line that is not already a t('...') call.
const tsIdx = args.indexOf('--ts');
const withTs = tsIdx >= 0;
if (withTs) args.splice(tsIdx, 1);
const root = args[0] ?? 'apps/web/src';

const SKIP_DIRS = new Set(['__tests__', 'i18n', 'test', 'styles', 'node_modules']);
// RDY-I18N-001: the Entrance is counted like everything else (it was skipped while it was still being extracted).
const SKIP_PATHS = [];
const SKIP_FILE = /(^Entrance|^SkipLink|^Door|^icons?\b|Icons?\.tsx$)/i;

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (SKIP_DIRS.has(name)) continue;
      const rel = relative(root, p).split(sep).join('/');
      if (SKIP_PATHS.includes(rel)) continue;
      walk(p, out);
    } else if (
      withTs &&
      name.endsWith('.ts') &&
      !/\.(test|d)\.ts$/.test(name) &&
      !/(generated|types|api-client)/.test(name)
    ) {
      out.push(p);
    } else if (name.endsWith('.tsx') && !/\.test\.tsx$/.test(name) && !SKIP_FILE.test(name)) {
      out.push(p);
    }
  }
  return out;
}

const letters = (s) => (s.match(/\p{L}/gu) ?? []).length;
const ATTR =
  /\b(?:aria-label|title|placeholder|alt|label|description|helpText)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;

const TS_LITERAL = /'([A-Z][^'\\\n]*(?:\\.[^'\\\n]*)*)'|`([A-Z][^`\n$]*)`/g;
const TS_SKIP_LINE =
  /^\s*(\/\/|\*|import )|\bt\(|console\.|new Error|\btype |aria-|data-|className|testid/;
export function countTs(src) {
  let n = 0;
  for (const line of src.replace(/\/\*[\s\S]*?\*\//g, '').split('\n')) {
    if (TS_SKIP_LINE.test(line)) continue;
    for (const m of line.matchAll(TS_LITERAL)) {
      const text = m[1] ?? m[2] ?? '';
      if (/[A-Za-z]{3,}\s+[A-Za-z]{2,}/.test(text) && !/[/:=]/.test(text)) n++;
    }
  }
  return n;
}

// RDY-I18N-002: text that is code, not prose. A `>` that closes a comparison or a generic (`a.length > 0 && b <`,
// `Record<string, X>`, `T[], columns`) lets the JSX-text pattern read source as text; those carry `&&`, `||`, `[]`,
// a leading comma or digit, or a quote-comma-newline. Technical literals are data, not copy: shell commands
// (`sr-cli ...`), URLs and invite examples, and the fixed network and language names listed below.
const CODE_LIKE = /sr-cli\s|&&|\|\||\[\]|^\s*[,\d]|'\s*,\s*\n/;
const TECHNICAL =
  /^\s*(?:sr-cli\s[\s\S]*|(?:https?:\/\/)?[\w-]+(?:\.[\w-]+)+(?:\/[\w./-]*)?|[\w-]+\/[\w./-]*)\s*$/;
const TECHNICAL_TOKENS = new Set(['mainnet', 'testnet', 'signet', 'regtest', 'curl', 'hTKzmak']);
const isCode = (text) =>
  CODE_LIKE.test(text) || TECHNICAL.test(text) || TECHNICAL_TOKENS.has(text.trim());

export function countFile(src) {
  let n = 0;
  // Attribute string literals.
  for (const m of src.matchAll(ATTR)) {
    const v = m[1] ?? m[2] ?? '';
    if (letters(v) >= 3 && !isCode(v)) n++;
  }
  // JSX text: text between '>' and '<' outside braces, at least 3 letters.
  const noComments = src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');
  // `(?<!=)>` leaves out the `>` of an arrow (`() => Promise<void>`); a `;` never occurs in UI text but does between
  // two generics (`useState<string>(null); const x = useRef<`), which the pattern would otherwise read as text.
  for (const m of noComments.matchAll(/(?<!=)>([^<>{}]*)(?=<)/g)) {
    const text = m[1].replace(/&[a-z]+;/g, ' ');
    if (letters(text) < 3 || text.includes(';') || isCode(text)) continue;
    // Skip TypeScript generics / arrows / comparisons that merely look like text.
    if (/[=;()]|=>/.test(text) && !/[A-Za-z]{3,}\s+[A-Za-z]{2,}/.test(text)) continue;
    n++;
  }
  return n;
}

const rows = [];
for (const file of walk(root)) {
  const text = readFileSync(file, 'utf8');
  const count = file.endsWith('.ts') ? countTs(text) : countFile(text);
  if (count > 0) rows.push({ file: relative(root, file).split(sep).join('/'), count });
}
rows.sort((a, b) => b.count - a.count || a.file.localeCompare(b.file));
const total = rows.reduce((s, r) => s + r.count, 0);

const width = Math.max(4, ...rows.map((r) => r.file.length));
for (const r of rows) console.log(`${String(r.count).padStart(5)}  ${r.file.padEnd(width)}`);
console.log(`TOTAL ${total}`);
if (jsonOut) writeFileSync(jsonOut, JSON.stringify({ total, files: rows }, null, 2) + '\n');
