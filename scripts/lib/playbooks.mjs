// RDY-DOC-003: the rules that tie alerts, Admin > System tiles and deploy/RUNBOOK.md together. Pure functions, so
// scripts/check-ops-files.test.mjs can feed them broken inputs. check-ops-files.mjs calls `playbookProblems` with
// the real files.

/** Same slug rule as scripts/check-doc-links.mjs (GitHub's heading anchors). */
export function slug(heading) {
  return heading
    .toLowerCase()
    .replace(/`/g, '')
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/\s/g, '-');
}

/** The anchors of a Markdown file's headings (fenced code is skipped). */
export function headingSlugs(md) {
  const out = new Set();
  let fence = false;
  for (const line of md.split(/\r?\n/)) {
    if (/^\s*```/.test(line)) fence = !fence;
    const h = !fence && /^#{1,6}\s+(.*?)\s*#*\s*$/.exec(line);
    if (h) out.add(slug(h[1]));
  }
  return out;
}

/** `[{ name, runbook }]` for every `- alert:` rule; `runbook` is the annotation value or null. */
export function alertRules(yml) {
  const parts = yml.split(/^\s*-\s+alert:\s*/m).slice(1);
  return parts.map((p) => {
    const name = p.split(/\r?\n/, 1)[0].trim();
    const rb = /^\s*runbook:\s*(\S+)/m.exec(p);
    return { name, runbook: rb ? rb[1] : null };
  });
}

const METRIC_PREFIX = /^(http|db|gateway|jobs|sr)_[a-z0-9_]+$/;

/**
 * Metric names a Markdown file mentions: backticked lower-case identifiers with an exported-metric prefix, label
 * selectors and PromQL call syntax cut off (`jobs_processed_total{outcome="failed"}` names `jobs_processed_total`).
 */
export function metricsNamedIn(md) {
  const found = new Set();
  for (const m of md.matchAll(/`([^`\n]+)`/g)) {
    // A whole inline expression: pick the identifiers out of it.
    for (const id of m[1].matchAll(/\b([a-z][a-z0-9_]*)\s*(?=\{|\[|\)|,|$|\s)/g)) {
      if (METRIC_PREFIX.test(id[1])) found.add(id[1]);
    }
  }
  return [...found];
}

const RUNBOOK_REF = /deploy\/RUNBOOK\.md#([a-z0-9][a-z0-9-]*)/g;

/** Playbook anchors referenced from source files (the Admin > System tile links). */
export function anchorsReferencedIn(text) {
  return [...text.matchAll(RUNBOOK_REF)].map((m) => m[1]);
}

/**
 * @param {object} i
 * @param {string} i.alertsYml        deploy/monitoring/alerts.yml
 * @param {string} i.runbookMd        deploy/RUNBOOK.md
 * @param {string[]} i.docsMd         other Markdown whose metric names are checked (OPERATOR.md)
 * @param {{file: string, text: string}[]} i.tileSources  non-test sources of the Admin tile links
 * @param {Set<string>} i.known       metric series the server exports
 * @param {string[]} i.required       anchors that must exist (the playbooks the wave promises)
 */
export function playbookProblems({
  alertsYml,
  runbookMd,
  docsMd = [],
  tileSources,
  known,
  required,
}) {
  const problems = [];
  const anchors = headingSlugs(runbookMd);

  for (const rule of alertRules(alertsYml)) {
    const m = rule.runbook && /^deploy\/RUNBOOK\.md#([a-z0-9][a-z0-9-]*)$/.exec(rule.runbook);
    if (!rule.runbook) problems.push(`alerts.yml: alert ${rule.name} has no runbook annotation`);
    else if (!m)
      problems.push(`alerts.yml: alert ${rule.name} runbook must be deploy/RUNBOOK.md#<anchor>`);
    else if (!anchors.has(m[1]))
      problems.push(
        `alerts.yml: alert ${rule.name} points at RUNBOOK.md#${m[1]}, which has no heading`,
      );
  }

  for (const t of tileSources) {
    for (const a of anchorsReferencedIn(t.text)) {
      if (!anchors.has(a)) problems.push(`${t.file}: tile link RUNBOOK.md#${a} has no heading`);
    }
  }

  for (const a of required) {
    if (!anchors.has(a)) problems.push(`RUNBOOK.md: missing the playbook "${a}"`);
  }

  for (const [file, md] of [
    ['RUNBOOK.md', runbookMd],
    ...docsMd.map((d, n) => [`doc ${n + 1}`, d]),
  ]) {
    for (const metric of metricsNamedIn(md)) {
      if (!known.has(metric)) problems.push(`${file}: names unknown metric ${metric}`);
    }
  }
  return problems;
}

/** Playbooks the handbook promises (RDY-DOC-003 and RDY-DOC-004). Anchors, in heading order. */
export const REQUIRED_PLAYBOOKS = [
  'server-down',
  'high-error-rate',
  'slow-requests',
  'database-pool-exhausted',
  'jobs-failing',
  'disk-pressure',
  'job-backlog',
  'voice-egress',
  'invoice-stuck',
  'storage-usage',
  'backup-missing-or-stale',
  'wal-archive-lag',
  'restore-drill-failed',
  'csp-reports-spike',
  'suspicion-case-opened',
  'takedown-notice-received',
  'data-subject-request-received',
  'suspected-compromised-staff-account',
  'lost-owner-second-factor',
  'disk-full',
  'restore-from-backup',
  'a-member-reports-csam',
];

/** Every deploy/*.md except OPERATOR.md itself must be linked from OPERATOR.md (RDY-DOC-002). */
export function operatorMapProblems(operatorMd, deployDocs) {
  const problems = [];
  for (const f of deployDocs) {
    if (f === 'OPERATOR.md') continue;
    if (!operatorMd.includes(`](${f}`)) problems.push(`deploy/OPERATOR.md does not link ${f}`);
  }
  return problems;
}

/**
 * RDY-DOC-005: the exit-gate status table of docs/13. A row is `| phase | gate | status | where |`. "Proven" and
 * "Partly" need a link in the last cell; "Not yet" needs an RDY id or the owner checklist; anything else is wrong.
 */
export function gateTableProblems(md) {
  const problems = [];
  const start = md.indexOf('## Exit gate status');
  if (start < 0) return ['docs/13-roadmap-and-phases.md has no "## Exit gate status" section'];
  const rest = md.slice(start + 1);
  const next = rest.search(/\n## /);
  const section = next < 0 ? rest : rest.slice(0, next);
  const rows = section
    .split('\n')
    .filter((l) => l.startsWith('|') && !/^\|\s*-/.test(l))
    .slice(1);
  if (rows.length === 0) problems.push('docs/13: the exit gate status table has no rows');
  for (const row of rows) {
    const cells = row
      .split('|')
      .slice(1, -1)
      .map((c) => c.trim());
    const [phase, , status = '', where = ''] = cells;
    if (cells.length !== 4) {
      problems.push(`docs/13 gate table: malformed row for "${phase}"`);
      continue;
    }
    if (/^(Proven|Partly)\b/.test(status)) {
      if (!/\]\([^)]+\)/.test(where))
        problems.push(`docs/13 gate table: "${phase}" is ${status.split(/\s/)[0]} without a link`);
    } else if (/^Not yet\b/.test(status)) {
      if (!/RDY-[A-Z0-9]+-\d+|owner-checklist/.test(where))
        problems.push(
          `docs/13 gate table: "${phase}" is not yet but names no RDY item or owner action`,
        );
    } else problems.push(`docs/13 gate table: "${phase}" status must be Proven, Partly or Not yet`);
  }
  return problems;
}
