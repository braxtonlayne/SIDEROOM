// Opens or updates the one issue of a failed SDK compatibility run (M-16, D-306), titled
// "SDK compat: <library> <version>", through the gh CLI (the workflow's GITHUB_TOKEN, `issues: write`).
// The repo's scripts/inventory-to-issues.mjs only files issues for unticked inventory items, so a run
// failure cannot go through it; this is the same dry-run-by-default shape.
//
//   node open-issue.mjs --report report/discord.js.xml [--create]
//
// Without --create it prints the issue it would open or update and exits 0.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/** @param {string} xml JUnit report written by run.mjs */
export function summarise(xml) {
  const suite = xml.match(/<testsuite name="([^"]+)" tests="(\d+)" failures="(\d+)"/);
  if (!suite) throw new Error('not a sdk-compat JUnit report');
  const failures = [
    ...xml.matchAll(/<testcase [^>]*name="([^"]*)"[^>]*><failure message="([^"]*)"/g),
  ].map((m) => ({ name: m[1], message: m[2] }));
  return { name: suite[1], tests: Number(suite[2]), failed: Number(suite[3]), failures };
}

export function issueFor(summary, runUrl = '') {
  const title = `SDK compat: ${summary.name}`;
  const body = [
    `The weekly SDK compatibility run failed ${summary.failed} of ${summary.tests} checks for **${summary.name}**.`,
    '',
    ...summary.failures.map((f) => `- \`${f.name}\`: ${f.message}`),
    '',
    runUrl ? `Run: ${runUrl}` : '',
    'The checks are tier 2 of docs/dev/sdk-compat.md.',
  ].join('\n');
  return { title, body };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const opt = (n) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
  const summary = summarise(readFileSync(opt('--report'), 'utf8'));
  if (summary.failed === 0) {
    console.log('all checks passed, no issue');
    process.exit(0);
  }
  const runUrl = process.env.GITHUB_SERVER_URL
    ? `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`
    : '';
  const issue = issueFor(summary, runUrl);
  if (!args.includes('--create')) {
    console.log(JSON.stringify(issue));
    process.exit(0);
  }
  const found = JSON.parse(
    execFileSync(
      'gh',
      [
        'issue',
        'list',
        '--state',
        'open',
        '--search',
        `"${issue.title}" in:title`,
        '--json',
        'number,title',
      ],
      { encoding: 'utf8' },
    ),
  ).find((i) => i.title === issue.title);
  if (found) {
    execFileSync('gh', ['issue', 'comment', String(found.number), '--body', issue.body], {
      stdio: 'inherit',
    });
  } else {
    execFileSync('gh', ['issue', 'create', '--title', issue.title, '--body', issue.body], {
      stdio: 'inherit',
    });
  }
}
