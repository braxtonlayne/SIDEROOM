# Integration handoff: integration/phase29 (wave 29)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p29-integ`, branch `integration/phase29`, base a382cd52 (main at PR #57 plus
the owner-approved ffmpeg.wasm and muxer downloads). Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p29-integ-target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo command at a time; `CMAKE` = the VS 2019 Build Tools copy for sr-sdk.
No Postgres (the password file `%TEMP%\sr_pg_pw.txt` is missing). Nothing pushed, no PR, nothing merged to main.

## Resume here

Slice 1 done: the four merges. Next: pnpm install + wasm build, then the integration notes (webcodecs bridge,
playable_key, licences, notices, inventory, CHANGELOG), regeneration, gates.

## Merges

1. `feat/p29-conv-webcodecs` (145f3c4b): clean.
2. `feat/p29-conv-remux` (7130b117): `THIRD_PARTY_NOTICES.md` and its `apps/web/public` copy; both sections kept
   (muxers, then ffmpeg.wasm). `check-notices-copy.mjs` identical.
3. `feat/p29-conv-worker` (7b21b44b): clean.
4. `feat/p29-tv-byos` (a7a79708): clean (generated OpenAPI files are regenerated at the end anyway).
