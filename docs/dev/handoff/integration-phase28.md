# Integration handoff: integration/phase28 (wave 28)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-integ`, branch `integration/phase28`, base 666e7579 (wave 27, fixed).
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-integ-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo
command at a time; `CMAKE` = the VS 2019 Build Tools copy (see integration-phase27.md) for sr-sdk. Scratch DB `sr_p28_integ`
(not created yet). Nothing pushed, no PR, nothing merged to main, no server started.

## Resume here

Status 11:40: the four merges are done and committed. Next: fmt, workspace clippy, then the P28 decisions
and integration notes, then the generated client, then gates.

The Postgres password file `%TEMP%\sr_pg_pw.txt` is missing: every database step is skipped until it is back.

## Merges

1. `feat/p28-imm-flags` (60a7650a): clean.
2. `feat/p28-byos-legal` (5386073f): `docs/15-threat-model.md` (byos-legal's T28-T30 renumbered T29-T31; the test expects T1-T31).
3. `feat/p28-byos-browser` (bf848b71): `sr-admin` settings (both `byos.*` blocks, one copy of each key), `routes/storage/mod.rs`,
   `mounts.rs` (`create` takes the policy and the admin deps), `route_matrix.tsv`, `authz_matrix.rs`.
4. `feat/p28-byos-catalog` (24eaaffa): `routes/storage/mod.rs`, `route_matrix.tsv`, `sr-server/src/app.rs`, `authz_matrix.rs`
   (TENANT 36; the wave-27 guild-mount seeding loop kept; the file tail rebuilt from the three branches).
