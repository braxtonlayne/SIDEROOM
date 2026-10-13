# Handoff: feat/threads-forums (LNG-THR-001..003, LNG-FRM-001) - COMPLETE

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/threads, branch feat/threads-forums (base integration/phase1).
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/threads-target. Scratch DB `sr_threads` (kept; password in %TEMP%\sr_pg_pw.txt, never commit).

## Done and verified
- sr-chat threads module (rules, service, memory repo, HookChain); forum settings on create/modify channel.
- sr-db migration 0110, Pg thread repo, parent-delete cascade, message_count bookkeeping.
- sr-events THREAD_* variants; sr-gateway dispatch, intents, THREAD_LIST_SYNC.
- sr-api routes/threads (start from message, standalone, forum/media posts, members, active/archived listings), thread branches for PATCH/DELETE /channels/{thread}, channel/message object fields (`thread` on the starter message), thread-aware MessagesDeps::access, search and read states include threads.
- sr-server: ThreadsDeps, activity hook chain, `threads.auto_archive` job (id 110, every 5 min), gateway_pg visibility, GUILD_CREATE `threads`.
- OpenAPI + api-client regenerated; inventory LNG-THR-001..003 and LNG-FRM-001 ticked.
- Gate: fmt --check, clippy -D warnings, cargo test --workspace, cargo deny check all pass; `--ignored --test-threads=1` passes on a fresh DB (sr_threads).
- Live run (scratchpad threads_live.mjs, 8102, sr_threads): 74 checks passed.

## Notes
- The shared `sr_test` DB currently has migration 130 applied by another branch, so `cargo test -- --ignored` there fails with `VersionMissing(130)` (sr_db::jobs test). Not a code fault; use a fresh DB or the integrated branch.
- Forum starter attachments and AutoMod on starter messages are not supported. Archived cursor is an ISO8601 timestamp.
- Message count is stored exactly, reported capped at 50; pinned forum posts (flag 1<<1) never auto-archive.
