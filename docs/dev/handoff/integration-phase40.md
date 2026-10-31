# Integration: phase 40 (close)

Branch `integration/phase40` = `integration/phase39` (PR #70) + `feat/p40-ops-docs` (OPS-DOC-001) + `feat/p40-ops-ci` (OPS-CI-001), merged `--no-ff` by the commander without conflicts. OPS-DEP-001 (tantivy 0.27) is not part of this run: it waits for 2026-11-17 (or the advisory trigger) and stays open with its date.

## Gates run on the merged tree (2026-10-31)

| Gate | Result |
|---|---|
| `check-doc-links.mjs`, `check-ops-files.mjs` | ok |
| js-yaml parse of every workflow | ok |
| `ci-scripts.test.mjs` (6), `threat-model.test.mjs` + `check-ops-files.test.mjs` (16) | pass |
| `pnpm -r typecheck` (incl. e2e and the activities) | ok |
| `build-web.mjs`, `check-bundle-budget.mjs`, `check-entrance-isolation.mjs` | ok |
| Vitest `src/help` + `src/i18n` | 67 pass |
| `i18n-audit.mjs` | 0 |
| prettier on the touched docs and help | ok |

No Rust changed in this wave, so the Rust gates of wave 39 stand.

## To watch on the first CI run

- The two new `continue-on-error` steps in `e2e.yml`'s `live` job (the bridge specs against the fake Discord, `convert.spec.ts` with ffmpeg): flip them to required once green (owner checklist section 3). Until then docs/13 rightly says the bridge browser specs are proven locally only.
- `flaky-tests.yml` `hunt-e2e` (new).
- `security-nightly.yml`: dispatched by hand once PR #70 is on main (GitHub only dispatches workflows present on the default branch); OPS-IMM-003 ticks after its first green run.

## Open

Every owner decision and download of the phase is in `docs/dev/owner-checklist.md` sections 4 and 5, each with its current default.
