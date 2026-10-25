# Handoff: feat/p23-operator-handbook (RDY-DOC-002 to 005)

Branch `feat/p23-operator-handbook`, base `integration/phase23`. No migrations, no downloads, no servers, no cargo build. Nothing pushed.

## Resume here

RDY-DOC-002, 004 and 005 are ticked. **RDY-DOC-003 stays unticked**: everything except "each playbook walked once against the smoke stack" is done, and that cannot be done on this machine (no Docker, no running stack). Each playbook says so in `deploy/RUNBOOK.md` ("written against prod-smoke's documented stack; walk pending"). To close it: follow each playbook below on a running stack (the `prod-smoke.yml` stack or a real host), fix the steps that differ, and replace the status line with the date and stack.

## Done

| Item | What | Proof that ran |
|---|---|---|
| RDY-DOC-002 | `deploy/OPERATOR.md`: map of INSTALL, GO-LIVE, UPGRADE, BACKUP, RUNBOOK, GARAGE, TRANSCODER, FAQ (and the owner checklist, lawyer brief, docs 09 and 12); daily, weekly, monthly checks; the settings an operator must know; every Admin > System tile and its playbook; the playbook index | `node scripts/check-ops-files.mjs` fails if OPERATOR.md is missing or does not link every other `deploy/*.md` |
| RDY-DOC-003 | Playbooks added to `deploy/RUNBOOK.md`: Job backlog, Voice egress, Invoice stuck, Storage usage, Backup missing or stale (the `backup_failed` alert), WAL archive lag, Restore drill failed, CSP reports spike, Suspicion case opened. `alerts.yml` already named an anchor for each of its five rules; the check now enforces it. Admin > System red tiles link to anchors | `check-ops-files.mjs` (alert has a `runbook` annotation of the form `deploy/RUNBOOK.md#anchor`, the heading exists, tile links resolve, 22 required anchors exist, every metric a playbook or OPERATOR.md names is exported); `scripts/check-ops-files.test.mjs` (10 tests, broken inputs fail); `p23playbooks.test.tsx` (2 tests) |
| RDY-DOC-004 | Incident playbooks: Takedown notice received, Data-subject request received, Suspected compromised staff account (points at the existing drilled section), Lost owner second factor, Disk full, Restore from backup, A member reports CSAM | Commands, settings and screens grepped against `crates/sr-cli/src/main.rs`, `crates/sr-admin/src/settings.rs`, `crates/sr-features/src/compliance.rs`, `docs/specs/moderation-compliance.md` and the admin i18n labels |
| RDY-DOC-005 | "Exit gate status" table at the top of `docs/13-roadmap-and-phases.md`: phases 0 to 7 and waves 20 to 24, status Proven / Partly / Not yet, evidence links | `check-ops-files.mjs` fails a Proven or Partly row without a link and a Not yet row without an RDY id or the owner checklist; `check-doc-links.mjs` passes |

Files: `deploy/OPERATOR.md` (new), `deploy/RUNBOOK.md`, `docs/13-roadmap-and-phases.md`, `scripts/check-ops-files.mjs`, `apps/web/src/admin/sections/System.tsx` (tile links), `docs/inventory/20-launch-readiness.md` (own ticks). Outside the Split row, all small and new: `scripts/lib/playbooks.mjs` (the pure rules, so they can be tested), `scripts/check-ops-files.test.mjs`, `apps/web/src/admin/__tests__/p23playbooks.test.tsx`, `apps/web/src/i18n/messages/en/p23-operator-handbook-1.ts` (two keys).

## Playbooks to walk (all "walk pending")

Alerts and tiles: Server down, High error rate, Slow requests, Database pool exhausted, Jobs failing, Disk pressure (older, unchanged), Job backlog, Voice egress, Invoice stuck, Storage usage, Backup missing or stale, WAL archive lag, Restore drill failed, CSP reports spike, Suspicion case opened.
Incidents: Takedown notice received, Data-subject request received, Suspected compromised staff account (its steps are the drilled "Compromised staff account" section, proven by `crates/sr-cli/tests/drills.rs`), Lost owner second factor, Disk full, Restore from backup, A member reports CSAM.

The drilled sections (Lost or suspect secret key, Compromised staff account) already had a Postgres test; nothing else did.

## Admin > System tile links

Answered: P23-2 (D-369), follow-up T-03 (done on `integration/phase23`: the links now open `/admin/runbook#<anchor>` in the same tab; the GitHub URL is gone).

`System.tsx` shows a "Playbook" link (new tab, to the file on the repository's `main` branch on GitHub) next to: a failing readiness probe (Server down), Dead jobs above 0 (Jobs failing), "none" for Last backup (Backup missing or stale) and Last restore check (Restore drill failed; "none" for the optional object snapshot is not red), a disk that is unreadable or has less than 20 percent free (Disk pressure; 20 percent is the default of `alerts.disk_percent`, the page does not know the live setting). The literals `deploy/RUNBOOK.md#...` in `System.tsx` are read by the check. Ran: `p23playbooks.test.tsx`, `admin.test.tsx`, `p10.test.tsx`, `p17redirectUri.test.tsx`, `p20preflight.test.tsx`, `p21cspReports.test.tsx` and `src/i18n` (all green), `tsc --noEmit` clean, eslint clean on the touched files, prettier clean.

## Honest limits

Answered: P23-2 (D-369), follow-up T-03 (the in-app runbook), and P23-3 (D-370), follow-up T-04 (the gauges and rules are RDY-OBS-001 in wave 24).

- **No Prometheus rule for most playbooks.** The server exports only nine metrics (`observability.rs`): request count and duration, in-flight, database pool, gateway sessions, job outcomes, rate limited, dispatch bytes. Job backlog, voice egress, invoice, storage, WAL, drill and suspicion have no metric, so I added no rules for them (a rule on a metric that does not exist is exactly what the check forbids). They are driven by Admin tiles and the `#alerts` health job; the playbooks and OPERATOR.md say so. A queue-depth gauge and a last-backup-age gauge would make "Job backlog" and "Backup missing or stale" real alerts; that is server work for a later wave.
- **Not walked** (see above). Commands were checked against code, not executed.
- **Exit-gate evidence** for Proven and Partly rows is taken from the inventory's recorded runs (prod-smoke run 39962011447 on PR #46; PR #48 head 49363978 green) and from handoffs; I could not look at GitHub from here. Phase 0 is Proven for the dev stack in CI (`exit-gate.spec.ts` in `e2e.yml`); the same flow on a real host is the first run of GO-LIVE.
- The tile link points at `https://github.com/braxtonlayne/SIDEROOM/blob/main/...` (hard-coded in `System.tsx`); if the repository is private or moves, the link needs the owner's URL or an in-app copy of the runbook.

## Decisions and questions for the owner

Answered: P23-1 (D-368), follow-ups T-01 (this branch) and T-02 (integrator: RDY-PAY-001 and RDY-LIB-001 in wave 24). Item 2 stays with the lawyer (RDY-OWN-001).

1. **Answered: P23-1 (D-368), follow-up T-01 done on this branch (doc 13 gate lines and table rows; RDY-PAY-001 and RDY-LIB-001 are wave 24, phase 6 is Not yet, phase 7 is Partly).** Was: gates with no RDY item. Phase 3 (a signet payment end to end), phase 4 (the 10-chapter Library run) and phases 6 and 7 (no gate is written) are "Not yet" with no RDY item behind them. They need either an item in a later wave or an owner decision that the gate is waived. Phase 4's mobile half is in the owner checklist's real-device pass only.
2. **Lawyer (D-336 (h)).** Still open (RDY-OWN-001). The CSAM playbook has steps 6 and 7 marked LAWYER and undefined (report where and when, preservation period, who may handle the material). Safe steps (do not copy the material, hold it in place, remove from view, ban with `child_safety`) are written. Ask the lawyer before the first invite.
3. **Linking OPERATOR.md.** Done by the integrator on `integration/phase23` (one line each in `deploy/INSTALL.md`, `deploy/GO-LIVE.md`, the README and `docs/README.md`). Was: those files did not yet link `deploy/OPERATOR.md` (not my files).
4. The integrator checklist should include running `node --test "scripts/*.test.mjs"`, which now covers `check-ops-files.test.mjs`.

## Gates run

- `node scripts/check-ops-files.mjs`: ok (36 known metric series).
- `node --test scripts/check-ops-files.test.mjs`: 10 of 10.
- `node scripts/check-doc-links.mjs`: ok.
- prettier (the main checkout's copy) on every changed file: clean.
- Web: tsc, eslint, 96 tests in 10 files green (list above). No Rust touched, so no cargo run.
