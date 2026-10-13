# Handoff: feat/p3-ci-docs

Branch `feat/p3-ci-docs` (from `integration/phase2`). Not pushed.

## Done
- **F-01** `.github/workflows/rust.yml`: `SR_TEST_DATABASE_URL` env (same `postgres:17` service as `DATABASE_URL`) and a step `cargo nextest run --workspace --exclude sr-storage --run-ignored only -j1` after the normal run (sr-storage's live S3 tests stay in `storage.yml`). Determinism fixes:
  - `crates/sr-db/tests/admin_repo.rs::suspend_revokes_...`: the real cause was the "LIKE wildcards are literal" assertion (`q = "%"` must be empty). `guild_admin_repo` deliberately leaves an account whose display name contains `%_`, so any reused database failed. It now asserts `<username>%` matches nothing and that a bare `%` only matches rows that literally contain `%`.
  - `crates/sr-db/tests/social_repo.rs`: snowflake worker ids are derived from the OS pid (was fixed 5/6, shared with other test binaries), so separate test processes do not mint the same timestamp+worker id. I could not reproduce a `dm_and_group_lifecycle` failure (25 consecutive runs on a dirty database passed, before and after), so this is hardening, not a confirmed fix.
- **F-27** `deploy/compose/README.md`. Garage flags are copied from `bootstrap.sh`; NOT checked against `garage --help` (no Docker on this machine), and the README says so.
- **F-28** `wasm_bindgen_test` modules in `sr-permissions-wasm` and `sr-theme-wasm` (+ dev-dependency `wasm-bindgen-test 0.3.79`, already in the lockfile). Verified with `wasm-pack test --headless --chrome` (2 tests each pass).
- **F-16, F-31, F-32, F-34, F-35, F-36** docs (05 sections 3, 4, 6.4, 7; 02 4.5; 03 section 2; 04 section 4; 06 section 4; 07 section 4; lounge-chat spec sections 10 and 11/12 prune wording). Ticked in `docs/dev/answers-followups.md`.

## Not done
- **F-33** skipped on purpose: it is all ingest-naming-conventions / LIB-ING captions, which the library-ingest agent owns.
- The "admin spec prune wording" in F-36 had no standalone admin text; prune is described in the lounge-chat spec members row and doc 05 section 4, both updated.

## Verification
fmt, clippy `-D warnings`, `cargo test --workspace` (935 passed, 0 failed), `cargo deny check` ok. Ignored suite (`cargo test --workspace --exclude sr-storage -- --ignored --test-threads=1`, 82 passed) green on a fresh database and on two consecutive reruns on the same database. All workflow YAML and `dev.yml` parse (js-yaml). nextest is not installed locally, so the exact CI command was run as the equivalent `cargo test` form.

## Owner decisions / notes
- CI uses the same database for the normal and ignored runs. If you prefer isolation, point `SR_TEST_DATABASE_URL` at a second database created in a step.
- F-01 text says to exclude nothing; I excluded `sr-storage` because its ignored tests need a live Garage.
