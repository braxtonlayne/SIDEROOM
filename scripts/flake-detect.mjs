#!/usr/bin/env node
// Flake detector (XC-TEST): run a test command N times and report which tests fail some runs but not others.
//
//   node scripts/flake-detect.mjs --runs 10 -- pnpm --filter @sideroom/web exec vitest run
//   node scripts/flake-detect.mjs --runs 5 --json out.json -- cargo nextest run -p sr-api
//
// A test is *flaky* when it fails in at least one run and passes in at least one other; one that fails every
// run is *broken*. Recognised output: vitest ("FAIL file > name" / "✓ name" lines) and cargo test/nextest
// ("test name ... FAILED", "FAIL [..] name"). Anything else is reported by exit code only. Exit 1 when any
// test is flaky or broken, so a nightly workflow can alert.
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// eslint-disable-next-line no-control-regex
const ANSI = /\u001b\[[0-9;]*m/g;

export function failedTests(output) {
  const out = new Set();
  for (const raw of output.split(/\r?\n/)) {
    const line = raw.replace(ANSI, '');
    let m;
    if ((m = /^\s*FAIL\s+(\S.*?)\s*$/.exec(line)) && !/^\[/.test(m[1]) && m[1].includes('>'))
      out.add(m[1]);
    else if ((m = /^\s*test (\S+) \.\.\. FAILED/.exec(line))) out.add(m[1]);
    // Playwright's list reporter: "✘  4 [chromium] › file:9:1 › title (1.2s)", with "(retry #1)" on a retry.
    else if ((m = /^\s*(?:✘|×)\s+\d+\s+(\[.+?)(?:\s+\([\d.]+m?s\))?\s*$/.exec(line)))
      out.add(m[1].replace(/\s+\(retry #\d+\)$/, ''));
    else if ((m = /^\s*FAIL \[\s*[\d.]+s\]\s+(\S.*)$/.exec(line))) out.add(m[1].trim());
  }
  return out;
}

export function classify(runs) {
  const names = new Set(runs.flat());
  const flaky = [];
  const broken = [];
  for (const n of names) {
    const fails = runs.filter((r) => r.includes(n)).length;
    (fails === runs.length ? broken : flaky).push({ name: n, failures: fails, runs: runs.length });
  }
  return { flaky, broken };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const argv = process.argv.slice(2);
  const dash = argv.indexOf('--');
  if (dash < 0 || dash === argv.length - 1) {
    console.error('usage: flake-detect.mjs [--runs N] [--json file] -- <command...>');
    process.exit(2);
  }
  const opts = argv.slice(0, dash);
  const cmd = argv.slice(dash + 1);
  const runs = Number(opts[opts.indexOf('--runs') + 1] ?? 5) || 5;
  const jsonOut = opts.includes('--json') ? opts[opts.indexOf('--json') + 1] : undefined;

  const results = [];
  let exitFailures = 0;
  for (let i = 1; i <= runs; i++) {
    const r = spawnSync(cmd[0], cmd.slice(1), {
      encoding: 'utf8',
      shell: process.platform === 'win32',
      maxBuffer: 256 * 1024 * 1024,
    });
    const failed = failedTests(`${r.stdout ?? ''}\n${r.stderr ?? ''}`);
    if (r.status !== 0) exitFailures++;
    results.push([...failed]);
    console.log(`run ${i}/${runs}: exit ${r.status}, ${failed.size} failing test(s)`);
  }

  const { flaky, broken } = classify(results);
  for (const f of flaky) console.log(`FLAKY  ${f.name} (${f.failures}/${f.runs} runs failed)`);
  for (const b of broken) console.log(`BROKEN ${b.name} (failed every run)`);
  if (exitFailures && !flaky.length && !broken.length)
    console.log(
      `${exitFailures}/${runs} runs exited non-zero but no test names were recognised in the output`,
    );
  if (jsonOut)
    writeFileSync(jsonOut, JSON.stringify({ runs, flaky, broken, exitFailures }, null, 2));
  if (flaky.length || broken.length || exitFailures) process.exit(1);
  console.log(`no flakes in ${runs} runs`);
}
