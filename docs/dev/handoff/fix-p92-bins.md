# fix-p92-bins: the base binaries rebuilt (SEC-AUD-407, D-920)

## Times
T = 11:47 (commander). Lock `bins` acquired 11:43, cargo 11:43:41 to 11:49:35, smoke test to 11:51, lock released 11:51. Nothing held past 12:12.

## Step 0
- Free space on C: 146 GB.
- `cargo metadata --offline --locked` passed (zip 9.0.5 was in the local registry; nothing fetched).
- `rustc 1.99.0 (b940084d7 2026-09-28)`, `cargo 1.99.0 (5f94df478 2026-08-27)`.

## Build
`CARGO_TARGET_DIR=.../p92-bins-target CARGO_PROFILE_DEV_DEBUG=0 cargo build -j 4 -p sr-server -p sr-cli --offline --locked` (dev profile), commit `498abf10ebf60a77c7f4fda3ecf149f0c6754c68`. 5 min 53 s, exit 0, 0 compiler warnings. No file of the repository changed.

## Smoke test (all four passed)
1. `/readyz` answered 200 within the first poll (well under 60 s).
2. Server log: no ERROR line and no panic; "migrations applied" present. Four WARN lines, all expected for a test stack (empty `server.secret_key`, in-memory storage).
3. `p92-base-bin/sr-cli.exe --version` printed `sr-cli 0.0.0`, exit 0.
4. Sizes against `p80-base-bin`: sr-server.exe 191,379,456 B vs 191,331,840 B (+0.02%); sr-cli.exe 48,353,792 B vs 48,264,192 B (+0.19%). Both within 10%.

Wave 93 stacks copy from `p92-base-bin` (`BUILT_FROM.txt` is there). `p80-base-bin` untouched.

## SEC-AUD-407 row text
Base binaries rebuilt from the wave 92 tree (`Cargo.lock` with zip 9.0.5, RUSTSEC-2026-0548) into `p92-base-bin`; server starts, migrations apply, `/readyz` 200, CLI runs. Done wave 92 (D-920).

## Changelog
None (nothing a member or operator sees).

## Owner questions
None. The only download was the pre-approved zip 9.0.5 (already in the registry; no fetch happened).

## Cleanup
Temporary target `p92-bins-target` (6.6 GB) deleted; scratch DB `sr_p92_bins_ui` dropped; server stopped (process tree killed, ports 8701 and 9401 free); lock released.
