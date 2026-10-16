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

8. `feat/p14-e2ee` (head 9186c67), merged last as bb721c6 with the owner's approval. Conflicts in `crates/sr-core/src/config.rs`, `crates/sr-gateway/src/conn.rs` and `docs/dev/server.md`, all resolved by keeping both sides: the config has `voice.shards` (default 1) and `voice.e2ee` (default false), the server doc lists both keys, and the gateway dispatches both op 8 `REQUEST_GUILD_MEMBERS` and op 101 `E2EE_MLS`.

After the merge:

- ea3c3ad: `zip` 9.0.2 (yanked) updated to 9.0.4 with `cargo update -p zip` (owner approved the network access).
- 2c0d041: `deny.toml` ignores RUSTSEC-2026-0173, with the reason written next to it. `proc-macro-error2` is unmaintained. It is a build-time proc macro, reached only through openmls_rust_crypto, hpke-rs and libcrux-sha3, and there is no safe upgrade.

## Fixes on the integration branch

- `ActivityDecisionBody`: devplat's `routes/activities/handlers.rs::DecisionBody` clashed with `routes/settings/applications.rs::DecisionBody`, because misc emptied `KNOWN_CLASHES`. The served document gave the Activities decision route the member-application body (`approve`, `reason`). Fixed by renaming the schema with `#[schema(as = ActivityDecisionBody)]`, then regenerating OpenAPI and `schema.d.ts`; the MSW handlers did not change. `openapi_names` caught it.

- 04189fc, `zip` 9.0.4 regression: `ArchiveOffset::Detect` no longer tries offset 0 first. It now scans every byte before the end-of-central-directory record for signatures, so `inspect_stored` downloaded the whole stored archive through `RangeReader` (it fetched 5.9 MB of a 5.0 MB archive, and `a_stored_archive_is_listed_from_its_index_not_downloaded` failed). The new `ingest::archive::zip_archive` tries `ArchiveOffset::Known(0)` first and falls back to `Detect` for archives with bytes in front (self-extractors). The CBZ reader and the EPUB inspector both use it, and a new test covers plain and prefixed archives. The cost is that the central directory is parsed twice in the common case, which is a few blocks.

## Gates on the final tree (after the e2ee merge, at 04189fc)

- `cargo fmt --check` clean. `cargo clippy --workspace --all-targets -j 2 -- -D warnings` clean (and again on sr-library after the fix).
- WASM: `cargo check -j 2 --tests --target wasm32-unknown-unknown` passes for each of sr-e2ee-wasm, sr-graph-wasm, sr-library-wasm, sr-markdown-wasm, sr-permissions-wasm and sr-theme-wasm. The getrandom `js`/`wasm_js` features and openmls `js` are under a `cfg(target_arch = "wasm32")` table in sr-e2ee-wasm only. `cargo tree -e features --workspace` (native) shows neither `js` nor `wasm_js` on getrandom 0.2 or 0.4, and only `default` on openmls.
- OpenAPI regenerated, `api-client generate` and `mocks:check` (826 operations): no diff. Route matrix with `SR_UPDATE_ROUTE_MATRIX=1`: no diff, no `?` or `other` cells. `openapi_names` 2 pass and `KNOWN_CLASHES` is still empty. `route_matrix` 5 pass.
- `cargo test --workspace -j 2 --no-fail-fast`: 2538 passed, 1 failed (the zip regression above), 379 ignored. After the fix, sr-library has 131 passed and 0 failed (one new test).
- Postgres suites, on a fresh scratch database that was dropped afterwards: sr-activities, sr-api, sr-cinema-social, sr-cli, sr-db, sr-library-social, sr-server, sr-storage, sr-tv, sr-userfeat and sr-watchparty, run with `--include-ignored --test-threads=1`. 1316 passed, 0 failed. sr-gateway has no ignored tests, and its voice_e2ee tests ran in the workspace run.
- `cargo deny check` (online): advisories, bans, licenses and sources all ok. The duplicate-version warnings are mostly openmls's older RustCrypto stack.
- Web: `pnpm install --offline --frozen-lockfile`, then the wasm build (including e2ee). `pnpm -r typecheck` and `pnpm -r lint` clean, prettier clean, i18n-verify-diff against main MISSING 0.
- apps/web vitest 3.2.7 `--maxWorkers=2`: 218 files, 2118 of 2118 pass on the first run.
- Package tests: wasm 14 (`SR_WASM_REQUIRED=1`, MLS included), markdown-react 45, api-client 6, bot 18, ui 278, embedded-app-sdk 6. All pass.
- Web build and bundle budget: all routes within budget (lounge 395.1 of 600 KB, developers 224.3 of 400 KB). The E2EE WASM (2.0 MB, 732.7 KB gzip) and its glue chunk are reached only through dynamic `import()`, from `@sideroom/wasm`'s lazy loader and from the media worker. No chunk pulls them in through static imports, and `index.html` does not reference them. The worker is emitted as an ES module (`worker.format: 'es'`). The only other worker is pdf.js, which is loaded by `?url` and does not use that setting.
- `check-doc-links`, `check-ops-files`, `check-dependabot` ok. `node --test scripts/*.test.mjs`: 12 pass.

## Earlier gates (tree at the ActivityDecisionBody fix, without e2ee)

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
- The first `cargo deny` run (without e2ee) used the local advisory cache only. The final run above was online.
- E2EE has never run in a real browser: no RTCRtpScriptTransform or encoded streams, no module worker loading the WASM. Both flags stay off (see `feat-p14-e2ee.md`). `wasm-pack test --headless --chrome` (CI's wasm job) was not run locally. Only `cargo check --tests` for wasm32 was run.
- After the fix commit, the whole workspace was not re-run (only sr-library, which is the only crate the fix touches; `zip_archive` is `pub(crate)`).
- 34 operationIds are duplicated in OpenAPI (31 on main; devplat adds `shelf`/`end`/`join`-style ids). No test guards this, and the generated client keys on paths, so it does not break anything today.
- The settings blob has no server validator, so `soundboardFavorites` (p14-web) is free-form, as are the other keys.
