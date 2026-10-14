#!/usr/bin/env node
// Inventory to GitHub issues (XC-CI-001). Dry run by default: prints one JSON object per unticked item that
// has no issue yet. With --create it calls `gh issue create` for each (needs `gh auth login`; the owner runs
// this, agents never create issues).
//
//   node scripts/inventory-to-issues.mjs                      # dry run, all unticked items
//   node scripts/inventory-to-issues.mjs --file 18 --limit 5  # only 18-cross-cutting, first five
//   node scripts/inventory-to-issues.mjs --create --milestones scripts/milestones.json
//
// Labels: area:<inventory file slug>, kind:<TAG> (FE, BE, OPS, ...), and phase:<n> when the milestone map
// gives one. `milestones.json` maps an inventory file prefix to a milestone title, for example
// {"01": "Phase 0", "18": "All phases"}; items in files that are not listed get no milestone.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { loadInventory } from './lib/inventory.mjs';

export function issueFor(item, milestones = {}) {
  const prefix = item.file.slice(0, 2);
  const area = item.file.replace(/^\d+-/, '').replace(/\.md$/, '');
  const labels = [`area:${area}`, ...item.tags.map((t) => `kind:${t}`)];
  return {
    id: item.id,
    title: `${item.id}: ${item.title}`.slice(0, 120),
    body: [
      `Inventory item **${item.id}** (${item.section || 'no section'}) from \`docs/inventory/${item.file}\`.`,
      '',
      item.title,
      '',
      'Work on a branch named with the ID (`feat/' +
        item.id +
        '-short-name`), tick the checkbox in the same PR, and see AGENTS.md section 4.',
    ].join('\n'),
    labels,
    milestone: milestones[prefix],
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const opt = (n) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
  const milestones = opt('--milestones')
    ? JSON.parse(readFileSync(opt('--milestones'), 'utf8'))
    : {};
  const only = opt('--file');
  const limit = Number(opt('--limit') ?? Infinity);
  const create = args.includes('--create');

  const items = loadInventory()
    .filter((i) => !i.done && (!only || i.file.startsWith(only)))
    .slice(0, limit);
  for (const item of items) {
    const issue = issueFor(item, milestones);
    if (!create) {
      console.log(JSON.stringify(issue));
      continue;
    }
    const existing = execFileSync(
      'gh',
      ['issue', 'list', '--search', `${item.id} in:title`, '--state', 'all', '--json', 'number'],
      {
        encoding: 'utf8',
      },
    );
    if (JSON.parse(existing).length) {
      console.log(`skip ${item.id}: issue exists`);
      continue;
    }
    const a = ['issue', 'create', '--title', issue.title, '--body', issue.body];
    for (const l of issue.labels) a.push('--label', l);
    if (issue.milestone) a.push('--milestone', issue.milestone);
    console.log(execFileSync('gh', a, { encoding: 'utf8' }).trim());
  }
}
