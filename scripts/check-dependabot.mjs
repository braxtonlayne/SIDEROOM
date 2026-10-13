#!/usr/bin/env node
// Dependabot sanity (XC-CI): every `directory:` in .github/dependabot.yml exists, every ecosystem has a
// schedule, and the commit message prefix keeps update PRs passing the PR title lint.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const text = readFileSync(join(root, '.github', 'dependabot.yml'), 'utf8');
const problems = [];

const blocks = text.split(/^ {2}- package-ecosystem:/m).slice(1);
if (blocks.length === 0) problems.push('no update blocks found');
for (const b of blocks) {
  const eco = b.split(/\r?\n/)[0].trim();
  const dir = /directory:\s*(\S+)/.exec(b)?.[1];
  if (!dir) problems.push(`${eco}: missing directory`);
  else if (!existsSync(join(root, dir === '/' ? '' : dir.replace(/^\//, '')))) {
    problems.push(`${eco}: directory ${dir} does not exist`);
  }
  if (!/schedule:/.test(b)) problems.push(`${eco}: missing schedule`);
  if (!/prefix:\s*\S/.test(b))
    problems.push(`${eco}: add commit-message.prefix so the PR title lint passes`);
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`dependabot ok (${blocks.length} ecosystems)`);
