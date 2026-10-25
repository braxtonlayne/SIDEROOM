// RDY-DOC-002/003/005: the playbook, handbook-map and gate-table rules of check-ops-files.mjs, fed broken inputs.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  alertRules,
  gateTableProblems,
  headingSlugs,
  knownMetrics,
  metricsNamedIn,
  operatorMapProblems,
  playbookProblems,
} from './lib/playbooks.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const known = new Set(['jobs_processed_total', 'http_requests_total']);
const alertsOk = `groups:
  - name: x
    rules:
      - alert: JobsFailing
        expr: sum(rate(jobs_processed_total[15m])) > 0
        annotations:
          runbook: deploy/RUNBOOK.md#jobs-failing
`;
const runbookOk =
  '# Runbook\n\n## Jobs failing\n\nThe `jobs_processed_total{outcome="dead"}` series.\n';
const base = {
  alertsYml: alertsOk,
  runbookMd: runbookOk,
  tileSources: [],
  known,
  required: ['jobs-failing'],
};

test('a consistent set has no problems', () => {
  assert.deepEqual(playbookProblems(base), []);
});

test('an alert without a runbook annotation fails', () => {
  const alertsYml = alertsOk.replace(/\s+annotations:\s+runbook:.*\n/, '\n');
  const p = playbookProblems({ ...base, alertsYml });
  assert.match(p.join('\n'), /JobsFailing has no runbook annotation/);
});

test('an alert pointing at a missing heading fails', () => {
  const alertsYml = alertsOk.replace('#jobs-failing', '#nope');
  assert.match(
    playbookProblems({ ...base, alertsYml }).join('\n'),
    /RUNBOOK\.md#nope, which has no heading/,
  );
});

test('a playbook naming a metric the server does not export fails', () => {
  const runbookMd = `${runbookOk}\nAlso watch \`jobs_queue_depth\`.\n`;
  assert.match(
    playbookProblems({ ...base, runbookMd }).join('\n'),
    /unknown metric jobs_queue_depth/,
  );
});

test('label selectors and PromQL do not hide a metric', () => {
  assert.deepEqual(metricsNamedIn('`sum(rate(jobs_processed_total[5m])) by (kind)`'), [
    'jobs_processed_total',
  ]);
  assert.deepEqual(metricsNamedIn('`jobs_processed_total{outcome="failed"}`'), [
    'jobs_processed_total',
  ]);
  assert.deepEqual(
    metricsNamedIn('`SR_VOICE__EGRESS_BUDGET_MBPS` and `voice.egress_budget_mbps`'),
    [],
  );
});

test('a tile link to a missing playbook fails, and a required playbook must exist', () => {
  const tile = { file: 'apps/web/src/admin/x.tsx', text: "href='deploy/RUNBOOK.md#gone'" };
  const p = playbookProblems({
    ...base,
    tileSources: [tile],
    required: ['jobs-failing', 'disk-full'],
  }).join('\n');
  assert.match(p, /tile link RUNBOOK\.md#gone has no heading/);
  assert.match(p, /missing the playbook "disk-full"/);
});

test('alertRules and headingSlugs read the shapes the files use', () => {
  assert.deepEqual(alertRules(alertsOk), [
    { name: 'JobsFailing', runbook: 'deploy/RUNBOOK.md#jobs-failing' },
  ]);
  assert.ok(
    headingSlugs('## A member reports CSAM\n```\n# not a heading\n```\n').has(
      'a-member-reports-csam',
    ),
  );
});

test('the handbook must link every other deploy doc', () => {
  assert.deepEqual(
    operatorMapProblems('[a](A.md) [b](B.md#x)', ['A.md', 'B.md', 'OPERATOR.md']),
    [],
  );
  assert.deepEqual(operatorMapProblems('[a](A.md)', ['A.md', 'B.md']), [
    'deploy/OPERATOR.md does not link B.md',
  ]);
});

const gate = (rows) =>
  `## Exit gate status\n\n| Phase | Gate | Status | Where |\n|---|---|---|---|\n${rows}\n\n## Phase 0\n`;

test('the gate table: proven needs a link, not yet needs an RDY id', () => {
  assert.deepEqual(
    gateTableProblems(
      gate('| 0 | g | Proven | [spec](../e2e/x.ts) |\n| 1 | g | Not yet | RDY-VOX-002 |'),
    ),
    [],
  );
  assert.match(
    gateTableProblems(gate('| 0 | g | Proven | it ran |')).join('\n'),
    /Proven without a link/,
  );
  assert.match(
    gateTableProblems(gate('| 1 | g | Not yet | someday |')).join('\n'),
    /names no RDY item/,
  );
  assert.match(
    gateTableProblems(gate('| 1 | g | Done | [x](y) |')).join('\n'),
    /must be Proven, Partly or Not yet/,
  );
  assert.match(gateTableProblems('# nothing').join('\n'), /no "## Exit gate status" section/);
});

test('the known metric set also reads the SFU metrics file (RDY-OBS-001)', () => {
  const obs = 'pub const JOBS_TOTAL: &str = "jobs_processed_total";';
  const voice = 'pub const EGRESS_BPS: &str = "sr_voice_egress_bits_per_second";';
  assert.ok(!knownMetrics([obs]).has('sr_voice_egress_bits_per_second'));
  assert.ok(knownMetrics([obs, voice]).has('sr_voice_egress_bits_per_second'));
  assert.ok(knownMetrics([obs, voice]).has('jobs_processed_total_bucket'));
  // The real tree: the alert rules name an SFU series and the check still passes.
  assert.match(
    readFileSync(join(ROOT, 'deploy/monitoring/alerts.yml'), 'utf8'),
    /sr_voice_egress_bits_per_second/,
  );
});

test('the real tree passes the whole check', () => {
  const out = execFileSync('node', [join(ROOT, 'scripts/check-ops-files.mjs')], {
    encoding: 'utf8',
  });
  assert.match(out, /ops files ok/);
});
