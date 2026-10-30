#!/usr/bin/env node
// UTL-SRC-001: the utility drawer's search corpus (apps/web/src/tools/corpus.json) is embedded into
// sr-tools-wasm with include_str!. This checks it: every tool has 10 to 30 phrases, no empty or duplicate
// strings (within a tool or across tools), unique ids and names, and aliases that do not shadow another
// tool's name or alias. The normalisation matches the Rust tokenizer closely enough for duplicates
// (lowercase, diacritics folded, punctuation to spaces).
// Usage: node scripts/check-tools-corpus.mjs [path/to/corpus.json]
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const MIN_PHRASES = 10;
export const MAX_PHRASES = 30;

export const normalize = (s) =>
  String(s)
    .normalize('NFD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();

/** Every problem found in a parsed corpus document, as readable strings (empty when it is clean). */
export function corpusProblems(doc) {
  const problems = [];
  if (!doc || !Array.isArray(doc.tools) || doc.tools.length === 0) {
    return ['corpus has no "tools" array'];
  }
  const ids = new Map();
  const names = new Map(); // normalised name or alias -> owner id
  const phrases = new Map(); // normalised phrase -> owner id
  for (const [i, t] of doc.tools.entries()) {
    const where = `tool #${i + 1} (${t?.id ?? '?'})`;
    if (typeof t?.id !== 'string' || !/^[a-z0-9]+(-[a-z0-9]+)*$/.test(t.id)) {
      problems.push(`${where}: id must be a kebab-case string`);
      continue;
    }
    if (ids.has(t.id)) problems.push(`${where}: duplicate id`);
    ids.set(t.id, true);
    if (typeof t.name !== 'string' || !normalize(t.name)) problems.push(`${where}: empty name`);
    const aliases = t.aliases ?? [];
    if (!Array.isArray(aliases)) problems.push(`${where}: aliases must be an array`);
    for (const label of [t.name, ...(Array.isArray(aliases) ? aliases : [])]) {
      const n = normalize(label ?? '');
      if (!n) {
        if (label !== t.name) problems.push(`${where}: empty alias`);
        continue;
      }
      const owner = names.get(n);
      if (owner && owner !== t.id) problems.push(`${where}: "${label}" is also a name or alias of ${owner}`);
      names.set(n, t.id);
    }
    const list = t.phrases;
    if (!Array.isArray(list) || list.length === 0) {
      problems.push(`${where}: no phrases`);
      continue;
    }
    if (list.length < MIN_PHRASES || list.length > MAX_PHRASES) {
      problems.push(`${where}: ${list.length} phrases, need ${MIN_PHRASES} to ${MAX_PHRASES}`);
    }
    for (const p of list) {
      const n = typeof p === 'string' ? normalize(p) : '';
      if (!n) {
        problems.push(`${where}: empty or non-string phrase`);
        continue;
      }
      const owner = phrases.get(n);
      if (owner) {
        problems.push(
          owner === t.id
            ? `${where}: duplicate phrase "${p}"`
            : `${where}: phrase "${p}" is also a phrase of ${owner}`,
        );
      }
      phrases.set(n, owner ?? t.id);
    }
  }
  return problems;
}

const here = dirname(fileURLToPath(import.meta.url));
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const file = resolve(process.argv[2] ?? join(here, '..', 'apps', 'web', 'src', 'tools', 'corpus.json'));
  let doc;
  try {
    doc = JSON.parse(readFileSync(file, 'utf8'));
  } catch (e) {
    console.error(`check-tools-corpus: cannot read ${file}: ${e.message}`);
    process.exit(1);
  }
  const problems = corpusProblems(doc);
  if (problems.length) {
    console.error(`check-tools-corpus: ${problems.length} problem(s) in ${file}`);
    for (const p of problems) console.error(`  - ${p}`);
    process.exit(1);
  }
  const n = doc.tools.reduce((a, t) => a + t.phrases.length, 0);
  console.log(`check-tools-corpus: ${doc.tools.length} tools, ${n} phrases, clean`);
}
