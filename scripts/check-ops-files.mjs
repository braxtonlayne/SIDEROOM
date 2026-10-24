#!/usr/bin/env node
// Cheap structural checks for the operations files (XC-OPS-007, XC-OPS-001). Fails CI when
//  - a Grafana dashboard is not valid JSON or queries a metric the server does not export,
//  - an alert rule queries a metric the server does not export,
//  - a shell script under deploy/scripts (or deploy/install.sh) does not parse (`sh -n`),
//  - the production Caddyfile lost a security-critical line,
//  - a tracked .sh, or a file a workflow or composite action runs as ./path, is not executable in git.
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const problems = [];

// Metrics the server exports: string constants in observability.rs plus the dispatch histogram.
const obs = read('crates/sr-api/src/observability.rs');
const exported = new Set(
  [...obs.matchAll(/pub const [A-Z_]+: &str = "([a-z_]+)";/g)].map((m) => m[1]),
);
const derived = (name) => [name, `${name}_bucket`, `${name}_sum`, `${name}_count`];
const known = new Set([...exported].flatMap(derived));

const metricsIn = (expr) =>
  [...expr.matchAll(/\b([a-z][a-z0-9_]*)\s*(?:\{|\[|\)|,|$|\s)/g)]
    .map((m) => m[1])
    .filter((n) => /^(http_|db_|gateway_|jobs_|sr_)/.test(n));

function checkExpr(where, expr) {
  for (const metric of metricsIn(expr)) {
    if (!known.has(metric)) problems.push(`${where}: unknown metric ${metric}`);
  }
}

const dashDir = 'deploy/monitoring/grafana/dashboards';
for (const f of readdirSync(join(root, dashDir)).filter((x) => x.endsWith('.json'))) {
  let d;
  try {
    d = JSON.parse(read(`${dashDir}/${f}`));
  } catch (e) {
    problems.push(`${f}: invalid JSON (${e.message})`);
    continue;
  }
  if (!d.uid || !d.title || !Array.isArray(d.panels) || d.panels.length === 0)
    problems.push(`${f}: needs uid, title and panels`);
  for (const p of d.panels ?? [])
    for (const t of p.targets ?? []) checkExpr(`${f} / ${p.title}`, t.expr);
}

for (const m of read('deploy/monitoring/alerts.yml').matchAll(/expr:\s*(.+)/g)) {
  checkExpr('alerts.yml', m[1]);
}

const shellScripts = [
  ...readdirSync(join(root, 'deploy/scripts'))
    .filter((x) => x.endsWith('.sh'))
    .map((f) => `deploy/scripts/${f}`),
  'deploy/install.sh',
];
for (const f of shellScripts) {
  try {
    execFileSync('sh', ['-n', join(root, f)], { stdio: 'pipe' });
  } catch (e) {
    problems.push(`${f}: ${e.stderr?.toString().trim() || 'does not parse'}`);
  }
}

// Executable bits. A script committed from Windows is mode 100644 and a workflow that runs it as `./path` stops
// with "Permission denied" (exit 126; deploy/compose/coturn/render-test-conf.sh in the first Phase 22 CI run).
// Every tracked `.sh`, and every tracked file a workflow or a composite action runs as `./path`, must be 100755
// in git: `git update-index --chmod=+x <file>` fixes it.
const modes = new Map(
  execFileSync('git', ['ls-files', '-s'], { cwd: root, encoding: 'utf8', maxBuffer: 64 << 20 })
    .split('\n')
    .filter(Boolean)
    .map((l) => {
      const [meta, path] = l.split('\t');
      return [path, meta.split(' ')[0]];
    }),
);
const mustRun = new Map(
  [...modes.keys()].filter((p) => p.endsWith('.sh')).map((p) => [p, 'a .sh']),
);
const runners = [
  ...readdirSync(join(root, '.github/workflows'))
    .filter((x) => /\.ya?ml$/.test(x))
    .map((f) => `.github/workflows/${f}`),
  ...readdirSync(join(root, '.github/actions'), { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => `.github/actions/${d.name}/action.yml`)
    .filter((p) => modes.has(p)),
];
for (const w of runners) {
  for (const m of read(w).matchAll(/(?:^|[\s;&|("'=])\.\/([A-Za-z0-9_][A-Za-z0-9_./-]*)/gm)) {
    if (modes.has(m[1]) && !m[1].startsWith('.github/actions/')) mustRun.set(m[1], `run by ${w}`);
  }
}
for (const [p, why] of mustRun) {
  if (modes.get(p) !== '100755')
    problems.push(`${p} (${why}) is not executable in git (mode ${modes.get(p)})`);
}

const caddy = read('deploy/compose/caddy/Caddyfile.prod');
for (const needle of [
  'Strict-Transport-Security',
  'handle /internal/*',
  'respond 404',
  'Content-Security-Policy',
  "frame-ancestors 'none'",
]) {
  if (!caddy.includes(needle)) problems.push(`Caddyfile.prod is missing: ${needle}`);
}

// The go-live checklist must exist and be reachable from the install guide (RDY-OWN-003).
if (!read('deploy/GO-LIVE.md').includes('## 9. The first invites'))
  problems.push('deploy/GO-LIVE.md is missing or lost its last step');
if (!read('deploy/INSTALL.md').includes('GO-LIVE.md'))
  problems.push('deploy/INSTALL.md does not link GO-LIVE.md');

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`ops files ok (${known.size} known metric series)`);
