#!/usr/bin/env node
// RDY-SEC-005: secret scan without a download. A regex set over the tracked files (or over the added
// lines of a diff in CI). It NEVER prints a matched value: findings are reported as file:line and the
// pattern name only. Known-safe hits live in scripts/secret-scan.allow.json (path + pattern + reason).
// Usage:
//   node scripts/secret-scan.mjs                  scan every tracked file
//   node scripts/secret-scan.mjs --diff <base>    scan only lines added since <base> (git diff <base>...HEAD)
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const ALLOW_FILE = join(ROOT, 'scripts', 'secret-scan.allow.json');
const MAX_BYTES = 2 * 1024 * 1024;

// A value that is obviously not a secret: empty, a template, a variable reference or a word that says so.
const PLACEHOLDER =
  /^(?:|["']{0,2}|0|1|true|false|none|null|<[^>]*>|\$\{?[A-Za-z_]\S*|%[A-Za-z_]+%|\{\{.*\}\}|.*(?:change[-_ ]?me|placeholder|example|replace|your[-_ ]|dummy|redacted|secret-here|todo|fixme|xxx+|\.\.\.|\*{3,}|test|dev|local|fake|sample|unused|not-a-secret).*)$/i;

/** Value after `=` or `:` with quotes, trailing comment and `,;` stripped. */
function cleanValue(raw) {
  let v = raw.trim();
  v = v
    .replace(/\s+#.*$/, '')
    .replace(/[,;]+$/, '')
    .trim();
  if (
    v.length >= 2 &&
    ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'")))
  ) {
    v = v.slice(1, -1);
  }
  return v;
}

/** True when the value is not a literal secret: too short, or built from a command, variable or template. */
function notLiteral(raw) {
  const v = cleanValue(raw);
  const quoted = /^["']([^"']*)["']/.exec(raw.trim());
  const first = quoted ? quoted[1] : v.split(/\s+/)[0];
  return first.length < 8 || /[$`(){}<>%]/.test(first) || PLACEHOLDER.test(first);
}

const WEAK_PASSWORD = /^(?:password|postgres|sideroom|pass|secret|user|admin|root|hunter2)$/i;

/**
 * Each pattern: name, a regex run per line, and an optional `reject(match)` that returns true when the
 * hit is a placeholder (so it is not a finding).
 */
export const PATTERNS = [
  { name: 'private-key-block', re: /-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----/ },
  { name: 'aws-access-key-id', re: /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/ },
  {
    name: 'github-token',
    re: /\b(?:gh[pousr]_[A-Za-z0-9]{36,255}|github_pat_[A-Za-z0-9_]{50,255})\b/,
  },
  {
    name: 'discord-token',
    re: /\b[MNO][A-Za-z0-9_-]{23,27}\.[A-Za-z0-9_-]{6,7}\.[A-Za-z0-9_-]{27,40}\b/,
  },
  {
    name: 'sr-secret-assignment',
    re: /\bSR_[A-Z0-9_]*(?:SECRET|PASSWORD|PASSWD|TOKEN|PRIVATE_KEY|API_KEY|SEED)[A-Z0-9_]*\s*[=:]\s*(\S.*)$/,
    reject: (m) => notLiteral(m[1]),
  },
  {
    name: 'password-in-url',
    re: /\b[a-z][a-z0-9+.-]{1,20}:\/\/[^\s:/@"'`<>]+:([^\s@/"'`<>]{3,})@[^\s"'`<>]+/i,
    reject: (m) => PLACEHOLDER.test(cleanValue(m[1])) || WEAK_PASSWORD.test(m[1]),
  },
  {
    name: 'password-query-param',
    re: /[?&;]password=([^&\s"'`<>;]{4,})/i,
    reject: (m) => PLACEHOLDER.test(cleanValue(m[1])) || WEAK_PASSWORD.test(m[1]),
  },
];

/** Path-level rule: an environment file that is not an example/template. */
export function isStrayEnvFile(path) {
  const base = path.split('/').pop() ?? '';
  if (!/^\.env(?:\..+)?$/.test(base)) return false;
  return !/\.(?:example|sample|template)$/.test(base);
}

/** Findings for one file's text: [{file, line, pattern}] (never the matched text). */
export function scanText(file, text) {
  const out = [];
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.length > 4000) continue; // minified blobs: not a place for hand-written secrets
    for (const p of PATTERNS) {
      const m = p.re.exec(line);
      if (!m) continue;
      if (p.reject?.(m)) continue;
      out.push({ file, line: i + 1, pattern: p.name });
    }
  }
  return out;
}

/** Scan added lines only. `added` is [{line, text}]. */
export function scanAdded(file, added) {
  const out = [];
  if (isStrayEnvFile(file)) out.push({ file, line: 1, pattern: 'env-file' });
  for (const { line, text } of added) {
    for (const f of scanText(file, text)) out.push({ file, line, pattern: f.pattern });
  }
  return out;
}

function globToRe(glob) {
  const esc = glob
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/\*\*/g, '\u0000')
    .replace(/\*/g, '[^/]*')
    .replace(/\u0000/g, '.*');
  return new RegExp(`^${esc}$`);
}

/** allow: [{path: glob, pattern: name or '*', reason}] */
export function applyAllowList(findings, allow) {
  const rules = allow.map((a) => ({ re: globToRe(a.path), pattern: a.pattern ?? '*' }));
  return findings.filter(
    (f) => !rules.some((r) => r.re.test(f.file) && (r.pattern === '*' || r.pattern === f.pattern)),
  );
}

export function loadAllowList(path = ALLOW_FILE) {
  if (!existsSync(path)) return [];
  const j = JSON.parse(readFileSync(path, 'utf8'));
  for (const a of j.allow ?? []) {
    if (!a.path || !a.reason) {
      throw new Error(`allow-list entry needs "path" and "reason": ${JSON.stringify(a)}`);
    }
  }
  return j.allow ?? [];
}

const git = (args) =>
  execFileSync('git', args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });

function looksBinary(buf) {
  return buf.subarray(0, 8000).includes(0);
}

export function scanTree() {
  const files = git(['ls-files', '-z']).split('\0').filter(Boolean);
  const findings = [];
  for (const f of files) {
    const abs = join(ROOT, f);
    let st;
    try {
      st = statSync(abs);
    } catch {
      continue;
    }
    if (!st.isFile()) continue;
    if (isStrayEnvFile(f)) findings.push({ file: f, line: 1, pattern: 'env-file' });
    if (st.size > MAX_BYTES) continue;
    const buf = readFileSync(abs);
    if (looksBinary(buf)) continue;
    findings.push(...scanText(f, buf.toString('utf8')));
  }
  return findings;
}

export function scanDiff(base) {
  const raw = git(['diff', '--unified=0', '--no-color', '--diff-filter=AM', `${base}...HEAD`]);
  const findings = [];
  let file = null;
  let next = 0;
  let added = [];
  const flush = () => {
    if (file) findings.push(...scanAdded(file, added));
    added = [];
  };
  for (const l of raw.split('\n')) {
    if (l.startsWith('diff --git ')) {
      flush();
      file = null;
    } else if (l.startsWith('+++ b/')) {
      file = l.slice(6);
    } else if (l.startsWith('@@')) {
      const m = /\+(\d+)/.exec(l);
      next = m ? Number(m[1]) : 0;
    } else if (l.startsWith('+') && !l.startsWith('+++')) {
      added.push({ line: next++, text: l.slice(1) });
    }
  }
  flush();
  return findings;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const i = process.argv.indexOf('--diff');
  const raw = i >= 0 ? scanDiff(process.argv[i + 1]) : scanTree();
  const findings = applyAllowList(raw, loadAllowList());
  if (findings.length) {
    for (const f of findings) console.error(`${f.file}:${f.line}  ${f.pattern}`);
    console.error(
      `\n${findings.length} possible secret(s). Values are never printed. Remove them, rotate if real, or add a reasoned entry to scripts/secret-scan.allow.json.`,
    );
    process.exit(1);
  }
  console.log(`secret-scan: clean (${i >= 0 ? 'diff' : 'tracked files'})`);
}
