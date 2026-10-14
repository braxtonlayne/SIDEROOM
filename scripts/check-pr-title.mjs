#!/usr/bin/env node
// PR title lint (XC-CI-002, AGENTS.md section 4): a title must name an inventory ID, or start with a
// conventional prefix for work that has none (docs:, chore:, ci:, test:, build:, revert:).
// Usage: PR_TITLE="feat(XC-PWA-001): manifest" node scripts/check-pr-title.mjs
import { fileURLToPath } from 'node:url';

const ID = /\b[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*-\d{3}\b/;
const PREFIX = /^(docs|chore|ci|test|build|revert|style)(\([^)]*\))?!?:\s*\S/;

export function checkTitle(title) {
  const t = title.trim();
  if (!t) return 'The title is empty.';
  if (PREFIX.test(t) || ID.test(t)) return null;
  return `"${t}" has no inventory ID (for example LNG-MSG-014) and does not start with docs:, chore:, ci:, test:, build: or revert:.`;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const problem = checkTitle(process.env.PR_TITLE ?? process.argv[2] ?? '');
  if (problem) {
    console.error(problem);
    process.exit(1);
  }
  console.log('PR title ok');
}
