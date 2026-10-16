#!/usr/bin/env node
// Cheap structural checks for the operations files (XC-OPS-007, XC-OPS-001). Fails CI when
//  - a Grafana dashboard is not valid JSON or queries a metric the server does not export,
//  - an alert rule queries a metric the server does not export,
//  - a shell script under deploy/scripts (or deploy/install.sh) does not parse (`sh -n`),
//  - the production Caddyfile lost a security-critical line.
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

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`ops files ok (${known.size} known metric series)`);
