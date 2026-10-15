# feat/p11-featmod-b: user-facing and content feature modules

Branch feat/p11-featmod-b (base integration/phase11). Migration 0760 only.

## Done
New crate `sr-userfeat` (eleven `Feature` impls with schemas, Postgres data layer, `Poster` port, scheduler), HTTP routes `sr-api/src/routes/userfeat`, wiring `sr-server/src/userfeat.rs` (system-user poster, `userfeat.tick` job id 760, leveling hook, watch party announcer hook).
- SCH, REM, BKM: backend plus web (composer clock button + dialog, header saved-items panel with bookmark notes, reminders and scheduled list; the old settings-blob store was replaced by a server-backed one).
- BDAY, LVL, RSS (SafeClient, seeded first fetch, backoff), LIBF, WPA, POLLT, SUGG, TICK: backend, routes, tests. Server-level settings (channels, roles, templates) appear in Server Settings > Features through the registry.
- `GET /guilds/{id}/modules` tells members which modules are on.

## Not done (UI)
No web UI yet for birthdays (user settings), leaderboard/rank, RSS feed manager, poll-template picker in the poll modal, suggestions, tickets. The API is complete and in the generated client. Web `build` and bundle budget were not run (wasm not built here); typecheck, lint, prettier and the lounge/i18n vitest suites pass except `channelPerms.test.ts`, which fails on the missing wasm build independent of this branch.

## Tests run
cargo: sr-userfeat 33 unit + 15 Postgres (`--test db -- --ignored --test-threads=1`); sr-server 5 Postgres (`userfeat_pg_tests`); sr-api userfeat 6 + route_matrix 3; fmt and clippy -D warnings clean. The ticket route is not exercised end to end (needs CREATE_PRIVATE_THREADS and a thread stack).

## Decisions
- Platform posts speak as the system user (id 1), which joins a server when a module first speaks; role pings work only for mentionable roles.
- Scheduled messages go out as the author through the message service (permissions, slowmode, AutoMod at send time).
- Leveling curve 5n^2+50n+100; birthdays use UTC; 29 Feb is celebrated on 28 Feb in non-leap years.
- One scheduler job (15 s) instead of one queued job per row, so edit and cancel are plain updates.

## Overlap with featmod-a's framework (reconcile on integration)
- Both branches register features in `sr-server/src/app.rs` (trivial conflict) and add route modules, OpenAPI and route-matrix rows.
- My modules use the plain `Feature` trait and their own tables (migration 0760); featmod-a added `0680_feature_state` and an actions port. Moving SUGG/TICK/BDAY/LVL state onto that keyed store and role changes onto their actions port is possible later; I did not, to keep working code. Role changes use `GuildBackend::assign_role` as the server owner (same idea as their RoleAssigner).
- Their per-server `/guilds/{id}/sr/features` list renders my schemas automatically.

## Owner questions
Is the system user joining servers as a visible member acceptable? Scheduled-message and reminder routes allow bot tokens (matrix BOT=allow); should they deny?
