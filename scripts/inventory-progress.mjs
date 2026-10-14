#!/usr/bin/env node
// Inventory progress summary (XC-CI-006). Prints a Markdown table of ticked vs total items per inventory
// file; in CI the output is appended to $GITHUB_STEP_SUMMARY. With --base <file.json> it also reports what a
// pull request ticked compared with a saved baseline (--write-json <file> saves one).
import { readFileSync, writeFileSync } from 'node:fs';
import { loadInventory } from './lib/inventory.mjs';

const args = process.argv.slice(2);
const opt = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined);

const items = loadInventory();
const byFile = new Map();
for (const it of items) {
  const row = byFile.get(it.file) ?? { done: 0, total: 0 };
  row.total++;
  if (it.done) row.done++;
  byFile.set(it.file, row);
}

const pct = (d, t) => (t === 0 ? '0%' : `${Math.round((d / t) * 100)}%`);
const lines = ['| Inventory | Done | Total | |', '|---|---:|---:|---:|'];
let done = 0;
for (const [file, r] of byFile) {
  lines.push(`| ${file.replace(/\.md$/, '')} | ${r.done} | ${r.total} | ${pct(r.done, r.total)} |`);
  done += r.done;
}
lines.push(`| **all** | **${done}** | **${items.length}** | **${pct(done, items.length)}** |`);

const base = opt('--base');
if (base) {
  const before = new Set(JSON.parse(readFileSync(base, 'utf8')).done);
  const ticked = items.filter((i) => i.done && !before.has(i.id)).map((i) => i.id);
  const unticked = [...before].filter((id) => !items.find((i) => i.id === id && i.done));
  lines.push('', `Ticked in this change: ${ticked.length ? ticked.join(', ') : 'none'}`);
  if (unticked.length) lines.push(`Unticked or removed: ${unticked.join(', ')}`);
}

const out = opt('--write-json');
if (out) writeFileSync(out, JSON.stringify({ done: items.filter((i) => i.done).map((i) => i.id) }));

console.log(lines.join('\n'));
