# Wave 47 builder "ci" (feat/p47-ci): E2E registration budget, legal stylesheet isolation scan

## Done
1. **E2E registration cap (CI only).** `.github/workflows/e2e.yml`: the main Playwright step is now two fresh-server runs, `tests/[a-f]` and `tests/[g-z]` (positional regex on the spec path, so a new spec lands in a half without editing the file; `--list` shows 22 + 40 files = the 62 non-tagged files, 116 + 780 tests). Own report, results dir and server log for the second half (`playwright-report-main-b`, `test-results-main-b`, `server-main-b.log`); artifact upload extended. A comment table documents the registration budget per run (about 12 / 15 call sites, showroom 2, gates 9; production limits in `register.rs` untouched). YAML parsed with js-yaml.
2. **Isolation check scans stylesheets.** `scripts/check-entrance-isolation.mjs`: plain-HTML surfaces (legal, error) now scan every stylesheet their documents link (and fail on a missing one or a sourceMappingURL); the static section scans every `.css` under `dist/entrance` not already scanned. Same deny-list as before (hand list + generated), a superset of the words the Playwright spec uses. Tests added in `scripts/check-entrance-isolation.test.mjs`: a planted Lounge/Cinema/Library/MiniPlayer in a legal stylesheet comment fails; an orphan stylesheet under `dist/entrance` fails. Verified against the real build too: a planted `/* Cinema */` in `dist/entrance/legal-*.css` fails the legal surface, and the restored dist is clean.
3. Inventory row `XC-CI-007` in `docs/inventory/18-cross-cutting.md`.

## Gates run
`node --test scripts/*.test.mjs` (114 tests, 0 fail), YAML parse, web build, `check-entrance-isolation` (clean), `check-bundle-budget` (clean), prettier --check on changed files, `playwright test --list --project chromium` for both halves and the tagged set. No cargo, no stack, no web source touched (no visual check needed).

## Not done
Not run on real CI: the split is untested there (needs a push; I did not push). Registration counts are static call-site counts, not measured.

## Owner questions
1. The task called the row XC-CI-001, but that ID is already the issue-generation script; I used XC-CI-007. OK?
2. Split by file-name letter (a-f / g-z) rather than by tag: no spec edits, but the halves are uneven in test count. Prefer a `@part2` tag or a third run?
3. Should the registration budget be enforced (e.g. a script counting `seedUser`/`openTheDoor` call sites per half)?

## Resume here
Nothing pending. After the first CI run, check that `server-main.log`/`server-main-b.log` and both reports appear in the artifact and no 429 occurs.
