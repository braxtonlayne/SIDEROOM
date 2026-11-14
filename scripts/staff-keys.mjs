#!/usr/bin/env node
// The staff key audit (SEC-AUD-187, D-663; docs/dev/entrance-isolation.md "Staff code").
//
//   node scripts/staff-keys.mjs --report            counts, and per fragment how many keys are staff / member / unnamed
//   node scripts/staff-keys.mjs --check             CI rule: exit 1 when a key lives in the wrong catalog
//   node scripts/staff-keys.mjs --write             move keys between apps/web/src/i18n/messages/en and en-staff
//   options: [srcDir]  default apps/web/src;  --modules <file.json>  take the staff module list from a build
//            (SR_STAFF_MODULES_OUT=<file> pnpm build writes it) instead of the source walk;  --json
//
// WHAT A STAFF KEY IS. A key is staff when it has at least one NAMING module and every naming module is staff-only.
// A module is staff-only when the build puts it in a staff chunk: the same rule as apps/web/vite/staffChunks.ts (the
// walk `sortModules` and the entry list live in scripts/lib/staff-graph.mjs and are shared). Here the module graph is
// read from source (static `import`/`export ... from`, dynamic `import()`, type-only imports skipped), rooted at the
// script of every apps/web/*.html; a module that nothing reaches is treated as a member module (never moved on its
// account). `--modules` takes the exact list from a real build instead (used once to prove the two agree).
//
// HOW A MODULE NAMES A KEY. Matching is deliberately generous (it reads the raw text, comments included, so an
// odd quote or a regex literal can never hide a use): a module names a key when
//   1. exact:    the key appears as a whole dotted token (`'admin.limits.title'`, in a `Record<string, MessageKey>`
//                table, an object field, a `t(...)` call);
//   2. prefix:   a dotted token ending in `.` that is followed by `${` or by a closing quote (`'admin.limits.' + x`,
//                `t(\`admin.nav.${id}\`)`) names every key that starts with it;
//   3. template: a template or string holding `${...}` parts after a dotted literal start (`admin.${s}.title`)
//                names every key matching it with `${...}` as `[A-Za-z0-9.]+`.
// Not seen: a key assembled from a variable with no literal start (`${ns}.title`, `ns + '.title'`): such sites are
// listed under "unresolved dynamic keys" by --report; pin the keys they name in scripts/staff-keys.pins.json
// (`{ "member": ["key", "prefix."] }`: a pinned key is never staff).
// Not naming modules: test files (*.test.*, *.spec.*, __tests__, src/test), type declarations, and the catalogs.
//
// WHAT --write DOES. A fragment of messages/en whose named keys are all staff moves whole with `git mv` to
// messages/en-staff/ (a key nobody names stays reported, not decided: it rides along only when the fragment moves
// whole); a mixed fragment keeps its member keys and its staff keys go to en-staff/<same file name>. A key in
// en-staff that a member module names goes back to messages/en. First-paint fragments (named in messages/en.ts and
// excluded in messages/en.rest.ts) are never touched. A second run changes nothing.
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, posix, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { STAFF_ENTRIES, sortModules } from './lib/staff-graph.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const slash = (p) => p.replaceAll('\\', '/');

// ---------------------------------------------------------------- fragments

/**
 * Splits a fragment file into `{ head, entries, foot }`. An entry is `{ key, text }`, `text` being its source from the
 * comment lines above it (which travel with it) to its closing comma. Only the shapes the catalog uses are accepted:
 * `'key': 'value',` (the value may wrap to the next line) with ' " or ` quoting.
 */
export function parseFragment(src) {
  const open = src.search(/export\s+default\s*\{/);
  if (open < 0) throw new Error('no `export default {` found');
  const start = src.indexOf('{', open) + 1;
  const entries = [];
  let i = start;
  let triviaStart = start;
  const n = src.length;
  const skipString = (j) => {
    const q = src[j];
    j++;
    while (j < n && src[j] !== q) j += src[j] === '\\' ? 2 : 1;
    return j + 1;
  };
  for (;;) {
    // trivia: whitespace and comments up to the next key quote or the closing brace
    while (i < n) {
      if (/\s/.test(src[i])) i++;
      else if (src.startsWith('//', i)) i = src.indexOf('\n', i) < 0 ? n : src.indexOf('\n', i) + 1;
      else if (src.startsWith('/*', i)) i = src.indexOf('*/', i) + 2;
      else break;
    }
    if (i >= n) throw new Error('unterminated fragment');
    if (src[i] === '}') break;
    if (src[i] !== "'" && src[i] !== '"') throw new Error(`unexpected text at offset ${i}`);
    const keyEnd = skipString(i);
    const key = src.slice(i + 1, keyEnd - 1);
    let j = keyEnd;
    while (/\s/.test(src[j])) j++;
    if (src[j] !== ':') throw new Error(`expected ':' after key ${key}`);
    j++;
    // value: strings, and anything up to the top-level comma
    let depth = 0;
    while (j < n) {
      const c = src[j];
      if (c === "'" || c === '"' || c === '`') j = skipString(j);
      else if (c === '{' || c === '(' || c === '[') (depth++, j++);
      else if (c === '}' || c === ')' || c === ']') {
        if (depth === 0) break; // the object's closing brace: last entry without a trailing comma
        depth--;
        j++;
      } else if (c === ',' && depth === 0) break;
      else j++;
    }
    const end = src[j] === ',' ? j + 1 : j;
    let from = triviaStart;
    // the entry's text starts at the first comment line of its trivia (blank lines stay with the file)
    const lead = src.slice(triviaStart, i);
    const k = lead.search(/\/[/*]/);
    from = k < 0 ? i : triviaStart + k;
    // go back to the start of that line
    while (from > triviaStart && src[from - 1] !== '\n') from--;
    entries.push({ key, text: src.slice(from, end) });
    i = end;
    triviaStart = end;
  }
  return { head: src.slice(0, start), entries, foot: src.slice(i) };
}

const LINE_END = (s) => (s.endsWith('\n') ? s : `${s}\n`);

/** Rebuilds a fragment from a parsed one with the given entries. */
export function renderFragment({ head, foot }, entries) {
  const body = entries.map((e) => LINE_END(e.text.replace(/^\n+/, '')));
  const h = head.endsWith('\n') ? head : `${head}\n`;
  return `${h}${body.map((b) => (b.startsWith('  ') || b.startsWith('\t') ? b : `  ${b}`)).join('')}${foot.replace(/^\s*/, '')}`;
}

function firstPaintFragments(messagesDir) {
  const names = new Set();
  const read = (f) =>
    existsSync(join(messagesDir, f)) ? readFileSync(join(messagesDir, f), 'utf8') : '';
  for (const m of read('en.ts').matchAll(/'\.\/en\/\{([^}]*)\}\.ts'|'\.\/en\/([\w-]+)\.ts'/g)) {
    for (const n of (m[1] ?? m[2]).split(',')) names.add(`${n.trim()}.ts`);
  }
  for (const m of read('en.rest.ts').matchAll(/'!\.\/en\/([\w-]+)\.ts'/g)) names.add(`${m[1]}.ts`);
  return names;
}

/** `{ dir: 'en' | 'en-staff', file, path, parsed, firstPaint }[]` for every fragment of both directories. */
export function loadFragments(messagesDir) {
  const first = firstPaintFragments(messagesDir);
  const out = [];
  for (const dir of ['en', 'en-staff']) {
    const d = join(messagesDir, dir);
    if (!existsSync(d)) continue;
    for (const file of readdirSync(d)
      .filter((f) => f.endsWith('.ts'))
      .sort()) {
      const path = join(d, file);
      out.push({
        dir,
        file,
        path,
        parsed: parseFragment(readFileSync(path, 'utf8')),
        firstPaint: dir === 'en' && first.has(file),
      });
    }
  }
  return out;
}

// ---------------------------------------------------------------- source scan

const isTestFile = (rel) =>
  /\.(test|spec)\.[cm]?[tj]sx?$/.test(rel) ||
  /(^|\/)__tests__\//.test(rel) ||
  /^test\//.test(rel) ||
  /\.d\.[cm]?ts$/.test(rel);

function walkSources(srcDir, rel = '', out = []) {
  for (const name of readdirSync(join(srcDir, rel)).sort()) {
    if (name === 'node_modules') continue;
    const r = rel ? `${rel}/${name}` : name;
    if (statSync(join(srcDir, r)).isDirectory()) walkSources(srcDir, r, out);
    else if (/\.[cm]?[tj]sx?$/.test(name)) out.push(r);
  }
  return out;
}

const IMPORT_RES = [
  /\bimport\s+(?!type\b)(?:[\w$*{}\s,]+?\s+from\s+)?['"]([^'"\n]+)['"]/g,
  /\bexport\s+(?!type\b)[\w$*{}\s,]*?\s*from\s+['"]([^'"\n]+)['"]/g,
];
const DYN_RE = /\bimport\(\s*['"]([^'"\n]+)['"]\s*\)/g;

function resolveImport(from, spec, files) {
  if (!spec.startsWith('.')) return null;
  const norm = posix.normalize(posix.join(posix.dirname(from), spec));
  for (const cand of [
    norm,
    ...['.ts', '.tsx', '.mts', '.js', '.jsx', '.mjs'].map((e) => norm + e),
    ...['/index.ts', '/index.tsx', '/index.js'].map((e) => norm + e),
  ])
    if (files.has(cand)) return cand;
  return null;
}

/** `{ importedIds, dynamicallyImportedIds }` per module (paths under srcDir, forward slashes). */
export function buildGraph(srcDir, textOf) {
  const files = new Set(walkSources(srcDir));
  const graph = new Map();
  for (const f of files) {
    const text = textOf(f);
    const imported = new Set();
    const dynamic = new Set();
    for (const re of IMPORT_RES)
      for (const m of text.matchAll(re)) {
        // `import { type A, type B } from` has no runtime import
        const clause = m[0].replace(/\s+from\s+['"][^'"]*['"]$/, '');
        if (/\{\s*(type\s+\w+\s*,?\s*)+\}$/.test(clause)) continue;
        const r = resolveImport(f, m[1], files);
        if (r) imported.add(r);
      }
    for (const m of text.matchAll(DYN_RE)) {
      const r = resolveImport(f, m[1], files);
      if (r) dynamic.add(r);
    }
    graph.set(f, { importedIds: [...imported], dynamicallyImportedIds: [...dynamic] });
  }
  return { files, graph };
}

/** The set of staff-only module paths (relative to srcDir). */
export function staffModuleSet(srcDir, textOf, htmlRoots, staffEntries = STAFF_ENTRIES) {
  const { files, graph } = buildGraph(srcDir, textOf);
  const toRel = (e) => e.replace(/^src\//, '');
  const staffRel = new Set(staffEntries.map(toRel));
  const isStaffEntry = (id) => staffRel.has(id);
  const roots = htmlRoots.filter((r) => files.has(r));
  const info = (id) => graph.get(id) ?? null;
  const { member } = sortModules(files, roots, info, isStaffEntry);
  // staff-only = reached from a staff entry and not in the member set; an unreachable module is a member module
  const reach = new Set();
  const stack = [...staffRel].filter((e) => files.has(e));
  while (stack.length) {
    const id = stack.pop();
    if (reach.has(id)) continue;
    reach.add(id);
    const node = graph.get(id);
    if (node) stack.push(...node.importedIds, ...node.dynamicallyImportedIds);
  }
  const staff = new Set([...reach].filter((id) => !member.has(id)));
  const orphans = [...files].filter((f) => !member.has(f) && !reach.has(f));
  return { staff, member, orphans, files };
}

function htmlRootsOf(webDir) {
  const roots = [];
  if (!existsSync(webDir)) return roots;
  for (const f of readdirSync(webDir).filter((x) => x.endsWith('.html'))) {
    for (const m of readFileSync(join(webDir, f), 'utf8').matchAll(
      /<script[^>]*\ssrc="([^"]+)"/g,
    )) {
      roots.push(m[1].replace(/^\/?src\//, ''));
    }
  }
  return roots;
}

// ---------------------------------------------------------------- naming

const TOKEN = /[A-Za-z0-9]+(?:\.[A-Za-z0-9]+)+/g;
const PREFIX = /([A-Za-z0-9]+(?:\.[A-Za-z0-9]+)*\.)(?=['"`])/g;
// a run of key characters and ${...} pieces that holds at least one ${...}
const TEMPLATE = /(?:[A-Za-z0-9.]|\$\{[^{}\n]*\})*\$\{[^{}\n]*\}(?:[A-Za-z0-9.]|\$\{[^{}\n]*\})*/g;
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** What one module's text names: exact dotted tokens, prefixes, template regexes, and unresolved dynamic sites. */
export function namesIn(text) {
  const exact = new Set(text.match(TOKEN) ?? []);
  const prefixes = new Set();
  for (const m of text.matchAll(PREFIX))
    if (m[1].includes('.') || m[1].length > 1) prefixes.add(m[1]);
  const templates = [];
  const unresolved = [];
  for (const m of text.matchAll(TEMPLATE)) {
    const s = m[0];
    const lead = s.match(
      /^[A-Za-z0-9]+(?:\.[A-Za-z0-9]+)*\.?(?=\$\{)|^[A-Za-z0-9]+(?:\.[A-Za-z0-9]+)+/,
    );
    if (!lead || !lead[0].includes('.')) {
      // starts with a placeholder (or no dotted start): only interesting when it has a literal dotted part
      if (/\.[A-Za-z][A-Za-z0-9]*/.test(s.replace(/\$\{[^{}]*\}/g, '\u0000'))) unresolved.push(s);
      continue;
    }
    const re = s
      .split(/(\$\{[^{}\n]*\})/)
      .map((p) => (p.startsWith('${') ? '[A-Za-z0-9.]+' : escapeRe(p)))
      .join('');
    templates.push(new RegExp(`^${re}$`));
  }
  return { exact, prefixes: [...prefixes], templates, unresolved };
}

/** A key -> naming modules map for `keys`, from every non-test module of srcDir outside the catalogs. */
export function nameKeys(keys, modules, pins = { member: [] }) {
  const named = new Map(keys.map((k) => [k, new Set()]));
  const byPrefix = new Map();
  const dynamicSites = [];
  for (const [mod, text] of modules) {
    const { exact, prefixes, templates, unresolved } = namesIn(text);
    for (const e of exact) named.get(e)?.add(mod);
    if (unresolved.length) dynamicSites.push([mod, unresolved]);
    if (prefixes.length || templates.length) byPrefix.set(mod, { prefixes, templates });
  }
  for (const [mod, { prefixes, templates }] of byPrefix) {
    for (const k of keys) {
      if (prefixes.some((p) => k.startsWith(p)) || templates.some((r) => r.test(k)))
        named.get(k).add(mod);
    }
  }
  for (const pin of pins.member ?? []) {
    for (const k of keys)
      if (k === pin || (pin.endsWith('.') && k.startsWith(pin)))
        named.get(k).add('<pinned member>');
  }
  return { named, dynamicSites };
}

// ---------------------------------------------------------------- the audit

/** @returns the classification of every key plus the per-fragment table. */
export function audit({
  srcDir,
  modulesFile,
  messagesDir = join(srcDir, 'i18n', 'messages'),
} = {}) {
  const webDir = resolve(srcDir, '..');
  const cache = new Map();
  const textOf = (rel) => {
    if (!cache.has(rel)) cache.set(rel, readFileSync(join(srcDir, rel), 'utf8'));
    return cache.get(rel);
  };
  let staff;
  let orphans = [];
  if (modulesFile) {
    staff = new Set(
      JSON.parse(readFileSync(modulesFile, 'utf8'))
        .filter((p) => p.startsWith('src/'))
        .map((p) => p.slice(4)),
    );
  } else {
    const r = staffModuleSet(srcDir, textOf, htmlRootsOf(webDir));
    staff = r.staff;
    orphans = r.orphans;
  }
  const fragments = loadFragments(messagesDir);
  const keys = fragments.flatMap((f) => f.parsed.entries.map((e) => e.key));
  const namingModules = walkSources(srcDir).filter(
    (r) => !isTestFile(r) && !r.startsWith('i18n/messages/') && !/^i18n\/messages\//.test(r),
  );
  const pinsFile = join(HERE, 'staff-keys.pins.json');
  const pins = existsSync(pinsFile) ? JSON.parse(readFileSync(pinsFile, 'utf8')) : { member: [] };
  const { named, dynamicSites } = nameKeys(
    keys,
    namingModules.map((m) => [m, textOf(m)]),
    pins,
  );
  const kind = new Map();
  for (const k of keys) {
    const mods = [...named.get(k)];
    if (mods.length === 0) kind.set(k, 'unnamed');
    else if (mods.every((m) => staff.has(m))) kind.set(k, 'staff');
    else kind.set(k, 'member');
  }
  return { fragments, kind, named, staff, orphans, dynamicSites, namingModules };
}

export function summarize(a) {
  const rows = [];
  const total = { staff: 0, member: 0, unnamed: 0 };
  for (const f of a.fragments) {
    const c = { staff: 0, member: 0, unnamed: 0 };
    for (const e of f.parsed.entries) c[a.kind.get(e.key)]++;
    rows.push({ fragment: `${f.dir}/${f.file}`, firstPaint: f.firstPaint, ...c });
    for (const k of Object.keys(total)) total[k] += c[k];
  }
  return { rows, total };
}

/** Violations of the CI rule. */
export function violations(a) {
  const out = [];
  for (const f of a.fragments) {
    for (const e of f.parsed.entries) {
      const k = a.kind.get(e.key);
      if (f.dir === 'en-staff' && k === 'member') {
        const who = [...a.named.get(e.key)].filter((m) => !a.staff.has(m));
        out.push(
          `${e.key} is in en-staff/${f.file} but member module(s) name it: ${who.slice(0, 3).join(', ')}`,
        );
      }
      if (f.dir === 'en' && !f.firstPaint && k === 'staff')
        out.push(
          `${e.key} (en/${f.file}) is only named by staff-only modules: run node scripts/staff-keys.mjs --write`,
        );
    }
  }
  return out;
}

/**
 * Computes the moves. `{ whole: [file], split: [{ file, keys }], back: [{ file, keys }] }`.
 */
export function plan(a) {
  const whole = [];
  const split = [];
  const back = [];
  const inStaff = new Set(a.fragments.filter((f) => f.dir === 'en-staff').map((f) => f.file));
  for (const f of a.fragments) {
    if (f.dir === 'en' && !f.firstPaint) {
      const kinds = f.parsed.entries.map((e) => a.kind.get(e.key));
      const staffN = kinds.filter((k) => k === 'staff').length;
      if (staffN === 0) continue;
      if (kinds.every((k) => k === 'staff') && !inStaff.has(f.file)) whole.push(f.file);
      else
        split.push({
          file: f.file,
          keys: f.parsed.entries.filter((e) => a.kind.get(e.key) === 'staff').map((e) => e.key),
        });
    } else if (f.dir === 'en-staff') {
      const keys = f.parsed.entries.filter((e) => a.kind.get(e.key) === 'member').map((e) => e.key);
      if (keys.length) back.push({ file: f.file, keys });
    }
  }
  return { whole, split, back };
}

function gitMv(from, to) {
  try {
    execFileSync('git', ['mv', from, to], { cwd: dirname(from), stdio: 'ignore' });
  } catch {
    renameSync(from, to);
  }
}

function removeFile(path) {
  try {
    execFileSync('git', ['rm', '-q', '-f', path], { cwd: dirname(path), stdio: 'ignore' });
  } catch {
    unlinkSync(path);
  }
}

/** Applies `plan`. Returns the counts. */
export function apply(a, messagesDir) {
  const p = plan(a);
  const counts = { keys: 0, whole: 0, split: 0, back: 0 };
  const byPath = (dir, file) => a.fragments.find((f) => f.dir === dir && f.file === file);
  const staffDir = join(messagesDir, 'en-staff');
  const memberDir = join(messagesDir, 'en');
  for (const file of p.whole) {
    const f = byPath('en', file);
    mkdirSync(staffDir, { recursive: true });
    gitMv(f.path, join(staffDir, file));
    counts.whole++;
    counts.keys += f.parsed.entries.length;
  }
  const moveEntries = (fromDir, toDir, file, keys) => {
    const from = byPath(fromDir === memberDir ? 'en' : 'en-staff', file);
    const set = new Set(keys);
    const moving = from.parsed.entries.filter((e) => set.has(e.key));
    const staying = from.parsed.entries.filter((e) => !set.has(e.key));
    const target = join(toDir, file);
    mkdirSync(toDir, { recursive: true });
    if (existsSync(target)) {
      const t = parseFragment(readFileSync(target, 'utf8'));
      writeFileSync(target, renderFragment(t, [...t.entries, ...moving]));
    } else {
      writeFileSync(target, renderFragment(from.parsed, moving));
    }
    if (staying.length === 0) {
      // nothing left: the source fragment goes away
      removeFile(from.path);
    } else writeFileSync(from.path, renderFragment(from.parsed, staying));
    counts.keys += moving.length;
  };
  for (const s of p.split) {
    moveEntries(memberDir, staffDir, s.file, s.keys);
    counts.split++;
  }
  for (const s of p.back) {
    moveEntries(staffDir, memberDir, s.file, s.keys);
    counts.back++;
  }
  return counts;
}

// ---------------------------------------------------------------- CLI

function main(argv) {
  const args = argv.slice(2);
  const take = (flag) => {
    const i = args.indexOf(flag);
    if (i < 0) return undefined;
    const v = args[i + 1];
    args.splice(i, 2);
    return v;
  };
  const modulesFile = take('--modules');
  const mode = ['--report', '--check', '--write'].find((m) => args.includes(m)) ?? '--report';
  const json = args.includes('--json');
  const srcDir = resolve(
    args.find((x) => !x.startsWith('--')) ?? join(HERE, '..', 'apps', 'web', 'src'),
  );
  const messagesDir = join(srcDir, 'i18n', 'messages');
  let a = audit({ srcDir, modulesFile });

  if (mode === '--report') {
    const { rows, total } = summarize(a);
    if (json) return console.log(JSON.stringify({ rows, total }, null, 2));
    console.log(
      `keys: ${a.kind.size}   staff: ${total.staff}   member: ${total.member}   unnamed: ${total.unnamed}`,
    );
    console.log(
      `staff-only modules: ${a.staff.size}   modules not reached from any entry: ${a.orphans.length}`,
    );
    for (const r of rows
      .filter((x) => x.staff || x.firstPaint === false)
      .filter((x) => x.staff > 0 || x.unnamed > 0))
      console.log(
        `  ${r.fragment.padEnd(34)} staff ${String(r.staff).padStart(4)}  member ${String(r.member).padStart(4)}  unnamed ${String(r.unnamed).padStart(4)}${r.firstPaint ? '  (first paint: never moved)' : ''}`,
      );
    const unnamed = [...a.kind].filter(([, k]) => k === 'unnamed').map(([k]) => k);
    if (unnamed.length) console.log(`unnamed keys (reported, not moved): ${unnamed.length}`);
    if (a.dynamicSites.length) {
      console.log(
        `unresolved dynamic keys (no literal start; pin in scripts/staff-keys.pins.json if they name keys): ${a.dynamicSites.length} module(s)`,
      );
      for (const [m, s] of a.dynamicSites.slice(0, 40))
        console.log(`  ${m}: ${s.slice(0, 3).join(' | ')}`);
    }
    return;
  }
  if (mode === '--check') {
    const bad = violations(a);
    if (bad.length) {
      console.error(
        `Staff keys FAILED:\n${bad
          .slice(0, 60)
          .map((b) => `  ${b}`)
          .join('\n')}`,
      );
      if (bad.length > 60) console.error(`  ... and ${bad.length - 60} more`);
      process.exit(1);
    }
    const { total } = summarize(a);
    console.log(
      `ok   staff keys: ${total.staff} staff, ${total.member} member, ${total.unnamed} unnamed; every key is in the right catalog`,
    );
    return;
  }
  const c = apply(a, messagesDir);
  console.log(
    `moved ${c.keys} key(s): ${c.whole} fragment(s) whole, ${c.split} split, ${c.back} moved back to en/`,
  );
  a = audit({ srcDir, modulesFile });
  const left = plan(a);
  if (left.whole.length || left.split.length || left.back.length) {
    console.error('a second pass would still move keys: the audit is not stable');
    process.exit(1);
  }
  console.log(`written under ${slash(relative(process.cwd(), messagesDir))}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  main(process.argv);
