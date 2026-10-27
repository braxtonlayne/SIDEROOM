# Integration handoff: integration/phase28 (wave 28)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-integ`, branch `integration/phase28`, base 666e7579 (wave 27, fixed), plus
integration/phase27 at 4973c0b8 merged in. Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-integ-target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo command at a time; `CMAKE` = the VS 2019 Build Tools copy (see
integration-phase27.md) for sr-sdk. Scratch DB `sr_p28_integ` (not created: no password file). Nothing pushed, no PR,
nothing merged to main, no server started.

## Resume here

Every step that needs no database is done (see "Gates"); the remaining work is the Postgres gates. The Postgres password
file `%TEMP%\sr_pg_pw.txt` was missing all day, so none of them ran. When it exists, in this order, one at a time, each
with `--include-ignored --test-threads=1` and BOTH `SR_TEST_DATABASE_URL` and `DATABASE_URL` set to a freshly created
scratch DB `sr_p28_integ` (never `sr_test`, never `postgres`):

1. `cargo test -j 2 -p sr-server --test authz_matrix -- --include-ignored --test-threads=1` (every group: imm-flags edited
   `run_group`; plus `storage_scopes_hold_over_http`, `storage_writes_hold_over_http`, `shelves_hold_over_http`).
2. `cargo test -j 2 -p sr-server --test immutable_rules -- --include-ignored --test-threads=1` (about 3 minutes warm).
3. `cargo test -j 2 -p sr-moderation --test storage_takedown --test takedowns -- --include-ignored --test-threads=1`,
   including the new `a_server_bucket_strikes_the_uploader_when_the_ledger_knows_them_else_the_owner` (P28-LEG-1,
   written at integration, compiles, never run: check it first).
4. `cargo test -j 2 -p sr-db -- --include-ignored --test-threads=1` (index_repo, shelf_repo, storage_legal,
   mount_uploads_repo, mount_attachments, mounts_repo, scopes_repo and the rest).
5. `cargo test -j 2 -p sr-server --lib -- --include-ignored --test-threads=1 voice_pg_tests` (never run since imm-flags
   edited it), then the same filter for `upload_jobs`, `storage_jobs`, `storage_index_pg_tests`.
6. The full `cargo test --workspace -- --include-ignored --test-threads=1`, only if it fits before 18:15 (wave 27's took
   well over an hour). Run it in the background with output to a log.
7. Drop `sr_p28_integ` and the agents' leftover scratch DBs `sr_p28_imm_flags`, `sr_p28_byos_browser`,
   `sr_p28_byos_catalog`.

Then tick the wave-28 rows of `docs/inventory/21-feature-phase.md` whose remaining proofs passed (each row says what is
missing), update this file and commit.

## Merges

1. `feat/p28-imm-flags` (60a7650a): clean.
2. `feat/p28-byos-legal` (5386073f): `docs/15-threat-model.md`. byos-legal's T28-T30 are renumbered T29-T31, and
   `scripts/threat-model.test.mjs` now expects T1-T31 (its failure message also printed no count; fixed).
3. `feat/p28-byos-browser` (bf848b71):
   - `sr-admin` settings: both `byos.*` blocks are kept, one copy of each key; imm-flags had not added the mount caps.
   - `routes/storage/mod.rs`.
   - `mounts.rs`: `create` takes imm-flags' `MountPolicy` and byos-browser's admin deps (`cap_for`).
   - `route_matrix.tsv`.
   - `authz_matrix.rs`.
4. `feat/p28-byos-catalog` (24eaaffa):
   - `routes/storage/mod.rs`, `route_matrix.tsv`.
   - `sr-server/src/app.rs`: the base's scope deps are kept; catalog's index store and shelf deps are added.
   - `authz_matrix.rs`: TENANT grows to 36 (`mount_upload_id`, `shelf_item_id`, `shelf_part_id`) and wave 27's
     guild-mount seeding loop is kept. The tail of the file was rebuilt from the three branches: the imm-flags
     `harness` module, then browser's `storage_writes_hold_over_http`, then catalog's `shelves_hold_over_http`. This
     also repaired an interleaving the previous merge had left.
5. `integration/phase27` (4973c0b8, the commander's PR #56 fixes):
   - `Mount::storage` allow_http: auto-merged.
   - CHANGELOG headings are now `## Phase N`, so wave 28's is `## Phase 28 - 2026-10-27`.
   - byos-browser's `S3Multipart`/`S3Admin` build plain reqwest clients (no `ClientOptions`, no `https_only`), and
     `Storage::s3` sets `with_allow_http` without client options, so the same mistake is not present elsewhere.

## Decisions and integration notes applied

- **P28-OPS-1:**
  - `immutable_rules.rs`: the LEGACY comment is reworded, and `the_deny_pattern_itself_is_pinned` asserts the
    one-entry list.
  - Decisions log: "Amended by P28-OPS-1" notes on D-249 and D-263, and the Phase 28 section with the rows P28-OPS-1
    and P28-LEG-1.
- **P28-LEG-1:**
  - `takedowns.rs::resolve_item`: on a guild bucket, the latest `completed` `storage_mount_uploads` row of the mount
    and key names the subject; otherwise the guild owner is the subject. The label ends "(uploaded by {user id})" or
    "(server bucket, owner)".
  - `notify_subjects` also DMs the guild owner (same text, not counted, no strike) through
    `takedown_mounts.rs::guild_mount_contact`. That helper shares a new `guild_owner_account` with
    `mount_owner_account`.
  - Postgres test added (not run).
  - Q20 in `docs/legal/lawyer-brief.md` and byos-legal's handoff line are reworded.
- **delisted_keys:** `PgIndexStore::delisted` now calls `storage_legal::delisted_keys` (fail closed on any error).
- **Index hooks:**
  - New trait `sr_storage::index::ObjectChanges` (implemented by `PgIndexStore` over `on_objects_deleted` and
    `on_objects_moved`) and an optional extension `routes::storage::IndexHooks { changes, index }`, registered in
    `app.rs`.
  - `DELETE .../objects/{key}` and `POST .../move` call it after the bucket call. A move then queues a run, and so
    does an upload's `complete` (the roadmap's "on upload complete").
  - A failure only logs, because the next run reaches the same rows.
- **member_shelves_in_guild:**
  - `GET /shelves/{section}` takes `?guild=<id>`. Inside a server's section it shows that server's own and attached
    shelves, plus the viewer's own and friends' shelves only when `ByosSettings::combine(platform, server)` allows it
    (`flags::member_shelves_allowed`). Other servers' shelves never show there.
  - Without the parameter nothing changes. The web does not send it yet; that is a follow-up for the server-section
    shelf screens.
  - Unit test: `member_shelves_in_guild_filters_the_server_section`.
- **Threat model:** renumbered (above).
- **Mount-cap settings:** one copy of each `byos.max_mounts_*` key.
- **Uniqueness checks:**
  - Job ids 138 (`storage.uploads_sweep`) and 1450 (`storage.index`) are unique.
  - Setting keys: the eight `byos.*` keys are unique, and sr-admin's catalog tests passed.
- **Build fixes after the merges:**
  - `NewItem.key: None` in two sr-server test initialisers that byos-legal's clippy had not covered.
  - `#[allow(clippy::too_many_arguments)]` on `delete_objects`.

## Gates

No database (all green):
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets -j 2 -- -D warnings`, sr-sdk included.
- `cargo deny --offline check`.
- `cargo test`, ignored tests skipped:
  - sr-core, sr-admin, sr-features, sr-storage, sr-voice-proto, sr-gateway, sr-voice, sr-moderation, sr-cinema,
    sr-library, sr-library-wasm, sr-media, sr-tv: 1116 passed, 0 failed.
  - sr-db, sr-api, sr-server: 905 passed, 0 failed.
  - sr-storage again after the phase27 merge: 138 lib plus the integration tests, including
    `an_http_endpoint_is_allowed_to_connect`.
- `route_matrix` (6) and `openapi_names` (2).
- OpenAPI regenerated (1017 operations), api-client `generate`, `mocks:generate`, and `mocks:check` passes.
- Web and repo checks:
  - `pnpm -r typecheck` and `pnpm -r lint`.
  - prettier on the whole repo (one moved spec reformatted).
  - `scripts/i18n-audit.mjs` (0) and `check-css-tokens`.
  - `node --test scripts/*.test.mjs` (58) and `deploy/scripts/media-lib.test.mjs` (7).
  - `check-ops-files`.
  - js-yaml parse of every workflow.
- Vitest and the web build: vitest --maxWorkers=2 on src/admin, cinema, i18n, lib, library, moderation, security, settings, storage, store, voice and help (incl. handbook.test.tsx, Phase 28 heading): 1503 of 1504 passed in one run; the one failure was a 30 s timeout under load in `admin/__tests__/takedowns.test.tsx`, and that file passed alone (9 of 9). `vite build` and `scripts/check-bundle-budget.mjs`: every chunk within budget, no pseudo-locale text.

With a database: none ran (the password file is missing). See "Resume here" for the list.

Not run in this wave by anyone: the Playwright specs (`voice-e2ee` on three engines, `noindex.spec.ts`, the
`moderation-gate` storage step, `byos-mount`), and the live `storage.yml` Garage case. CI runs these.
