# Handoff: integration/phase14

Branch `integration/phase14` (base `main` at ac99611). Local only; nothing pushed, no PR.

## Merged (merge commits, in this order)

1. `feat/p14-answers`: clean.
2. `feat/p14-web`: clean.
3. `feat/p14-fu-ops`: clean. `deploy/install.sh` is now 100755 in git; the `deploy/scripts/*.sh` modes are unchanged (all 100755).
4. `feat/p14-fu-gw`: conflict in `docs/dev/answers-followups-p13.md` (H-44 and H-45 ticked on different branches). Resolved with the tick-union script, so both are ticked.
5. `feat/p14-misc`: clean. It already contained fu-gw.
6. `feat/p14-lounge-rest`: clean.
7. `feat/p14-devplat`: conflict in `packages/api-client/src/mocks/handlers.generated.ts`. Regenerated it from the merged OpenAPI. `openapi.json` and `schema.d.ts` regenerated identical to the textual merge.

**Not merged: `feat/p14-e2ee`.** The coordinator asked for it to go in last, but the merge command was refused by the permission classifier. The user has to allow it.

## Fixes on the integration branch

- `ActivityDecisionBody`: devplat's `routes/activities/handlers.rs::DecisionBody` clashed with `routes/settings/applications.rs::DecisionBody`, because misc emptied `KNOWN_CLASHES`. The served document gave the Activities decision route the member-application body (`approve`, `reason`). Fixed by renaming the schema with `#[schema(as = ActivityDecisionBody)]`, then regenerating OpenAPI and `schema.d.ts`; the MSW handlers did not change. `openapi_names` caught it.

## Gates (tree at the fix commit, without e2ee)

- `cargo fmt --check`: clean. `cargo clippy --workspace --all-targets -j 2 -- -D warnings`: clean.
- `cargo test --workspace -j 2 --no-fail-fast`: 2500 passed, 1 failed (`openapi_names`, fixed above), 379 ignored. After the fix, `openapi_names` (2) and `route_matrix` (5) pass.
- Postgres suites, on a fresh scratch database that was dropped afterwards: sr-activities, sr-api, sr-cinema-social, sr-cli, sr-db, sr-library-social, sr-server, sr-storage, sr-tv, sr-userfeat and sr-watchparty, run with `--include-ignored --test-threads=1`. 1316 passed, 0 failed. The `s3_live` tests skip themselves when no endpoint is set.
- Route matrix regenerated (`SR_UPDATE_ROUTE_MATRIX=1`): no diff, no `?` or `other` cells. Migration versions are unique (0803, 0876 and 0930 are new).
- `cargo deny --offline check`: bans, licenses and sources ok. **Advisories fail** because `zip 9.0.2` is yanked. That lock entry is unchanged from `main`, so p14 did not introduce it. The fix is `cargo update -p zip`, which needs network access and was not run.
- Web: `pnpm install --offline --frozen-lockfile` (apps/web resolves vitest 3.2.7), wasm build, `pnpm -r typecheck` and `pnpm -r lint` clean, prettier clean, i18n-verify-diff against main MISSING 0, `mocks:check` up to date (826 operations).
- apps/web vitest `--maxWorkers=2`: 2097 of 2098 pass. The failure is `p13entrance.test.tsx` "uploads a picture…", which timed out waiting 25 s on the first, cold run. It also failed once on its own after that run, then passed in three warm runs (twice as the whole file, once with `-t`). It is a cold-start or load flake, not a logic failure.
- Package tests: api-client 6, markdown-react 45, wasm 9 (`SR_WASM_REQUIRED=1`), bot 18, ui 278, embedded-app-sdk 6. All pass.
- Web build and bundle budget: all routes within budget (lounge 393.6 of 600 KB, developers 223.7 of 400 KB).
- `check-doc-links`, `check-ops-files`, `check-dependabot` ok. `node --test scripts/*.test.mjs`: 12 pass (install tests included).
- Shell: shellcheck is not installed. I read the `install.sh` diff by eye: expansions are quoted, there is no `ls | grep` and no `local` assignment from a command substitution. CI's shellcheck covers only `deploy/scripts/*.sh`, and none of those changed.

## Not verified

- Playwright, real browsers and live S3 were not run.
- `cargo deny` advisories were checked against the local cache only (offline).
- e2ee is not merged, so none of the gates above cover it.
- 34 operationIds are duplicated in OpenAPI (31 on main; devplat adds `shelf`/`end`/`join`-style ids). No test guards this, and the generated client keys on paths, so it does not break anything today.
- The settings blob has no server validator, so `soundboardFavorites` (p14-web) is free-form, as are the other keys.
