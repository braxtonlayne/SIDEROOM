# Wave 64 handoff: resync (SEC-AUD-169, D-646), the build

Branch `fix/p64-resync`, base `integration/phase64` at `9949a4ba`. Design: `fix-p63-resync.md`. Nothing was dropped: all six items are built, including the tail (a), (b) and (c). One known limit is listed under "Not done / limits".

## Done (commits)

| commit | what |
|---|---|
| `0c5feb12` | Server half: `crates/sr-gateway/src/resync.rs` (new), `hub.rs`, `lists.rs` (`changes_snapshot`), `session.rs`/`conn.rs` (stored READY user), `voice.rs` (epoch bump), tests. |
| `d12e5639` | Web: `store/reducer.ts`, `store/store.ts`, `lounge/threads/threadStore.ts`, `moderation/reports.tsx`, `lounge/announcements/useActiveAnnouncements.ts`, five test files. |
| `2746067e` | `docs/05-api-and-gateway.md` (6.3 event name, 6.4 resync paragraph), generated `gatewayDocs.generated.ts`, bot paragraph in `developers/docsContent.ts`. |

What it does:

1. **Server topic** (`on_guild_lag`): after `schedule_list_refresh(guild, true)` (unchanged, first) the guild is queued. The worker sends each live non-bot session of the guild its member's own `GUILD_CREATE` (`guild_create_body`, one build per member however many sessions), `THREAD_LIST_SYNC` when there is one, then `SR_RESYNC {"scope":"guild","guild_id"}`, in one synchronous block through `Session::deliver` (frames are adjacent in the session's sequence).
2. **User topic** (`on_user_lag`): after the re-check, cache drops (unchanged, first) the user is queued. The worker builds all guild bodies first, then delivers `READY` (same `session_id`, built by `ready_body`, factored out of `create_session`) and each guild back to back. A revoked session is never sent anything: `send_recheck()` is queued on its socket before the resync is, the connection handles `Out::Recheck` first (FIFO) and closes 4004; the worker also checks `is_attached` and registration at delivery.
3. **Queue** (`resync.rs`): `Key::{Guild,User,Platform}`, a waiting map with arrival order, floor from the **end** of the previous round (a lag during a round waits the floor after it), a lag on a key already waiting adds nothing (that round covers it), bound 256 with the oldest dropped under a `warn!`, one worker, a token-bucket reservation (burst = rate = 40/s) per session told, `GUILD_CREATE_CONCURRENCY` (4) builds in flight. Figures are `GatewayConfig` fields with a `Debug` line each: `resync_floor` 30 s, `resync_platform_floor` 60 s, `resync_sessions_per_second` 40, `resync_queue_max` 256.
4. **Snapshot race**: `lists::changes_snapshot(&DomainEvent)` (everything except messages, edits, deletes, reactions, pins, poll votes, typing, presence) bumps a per-guild epoch in `State` at the start of `on_guild_event`; the voice, stage, sound and status dispatches (`dispatch_guild`, `dispatch_voice_state`) bump it too. The worker reads the epoch before the build and compares it under the state lock in the same step as the delivery; if moved it rebuilds, at most twice, then delivers and queues the key once more. Test kept active (not `#[ignore]`).
5. **Web**: `SR_RESYNC` guild in `reducer.ts`: `staleWindows(s, only)` (factored out of READY; pending sends kept), typing of the guild's channels dropped; `store.ts` `applyDispatch` refetches the member list if `memberIdsByGuild[guild]` exists, with `loadMembers(guild, {replace: true})`. A second READY with the same session id keeps the sequence counter (`client.ts:231-246` already did; test added, no code change).
6. **Tail**: (a) platform marker after `reconcile_memberships`; `ReportsFlagSync` (`reports.tsx`) refetches flags, `useActiveAnnouncements` refetches notices. (b) members replaced on a resync refetch (`putMembers(.., replace)`, set by `sr_replace` on the `GUILD_MEMBERS_CHUNK` the web builds itself, only when the REST page is under 1000 so a partial roster never replaces); READY removes `guilds` entries not listed via `dropGuild` (the GUILD_DELETE cleanup, shared), each with a test. (c) threads: `threadStore` records the ids named by the newest `GUILD_CREATE`/`THREAD_LIST_SYNC` of a guild and `SR_RESYNC` prunes the guild's active (non-archived) threads that were not named (this also works when the server sends no `THREAD_LIST_SYNC` because there are no threads: it sends none for an empty list, `gateway_pg.rs:1172`).

No route, response shape, migration or `openapi.json` change. No UI. No new import in any first-paint file (`reducer.ts`, `store.ts`, `reports.tsx` gained none); I did not build the bundle to measure it.

## Not done / limits

- Nothing from the six items was dropped.
- The READY a resync sends carries `user` from `Session::user_json`, set at identify/resume and refreshed on every credential re-check (the lag path queues one first, so it is normally a few ms old, never older than `session_recheck` = 60 s). The worker does not wait for the re-check to finish. A change to the member's own profile whose event was lost can therefore be a moment late in READY's `user`. Owner question 1.
- After a **user**-topic resync the web gets READY + GUILD_CREATEs but no `SR_RESYNC`, so it does not refetch member lists (as designed: the design's user path is exactly a reconnect's READY).
- GUILD_CREATE members are still merged (`reduceGuildPeople`); only the REST refetch after a guild resync replaces.
- `threadStore.named` keeps one small id set per guild from the last GUILD_CREATE (cleared only by an `SR_RESYNC`).
- Bundle budget not run (no first-paint import added).

## Every "assumed" of the design, checked

| assumed | found |
|---|---|
| gateway docs generator exists | `scripts/lib/gateway-docs.mjs` (writes double quotes; run prettier on the output, as I did). A test fails when the generated file is stale. |
| `store/reducer.ts` is in the Lounge chunk | not measured; I added no import to it or to `store.ts`, `reports.tsx`, `useActiveAnnouncements.ts`. |
| READY `user` is available to the worker | **No**: `Session` did not keep it. Stored on the session (identify, resume, re-check), see limits. |
| snapshot epoch is enough | the test forces the race (build blocked in `guild_payload`, `ChannelCreated` handled, build released): the stale snapshot is delivered without the epoch, the rebuilt one with it. Voice state changes were not covered by the design's event list; covered through the dispatch helpers. |
| composer is not remounted by a store update | not checked (not needed: no UI change, the resync swaps data like a reconnect does). |
| member list is "loaded" when `memberIdsByGuild[guild]` exists | read: `loadMembers` is the only REST loader, GUILD_CREATE members also fill it for small guilds; so a small guild counts as loaded and is refetched. Fine, one request. |
| `fetchMembers` returns the whole roster | **No**: it is one page of 1000. Replace is applied only when the answer has fewer than 1000 rows. |
| INVALID_SESSION path unchanged | untouched. |

## Tests

Rust (`cargo test -j 4 -p sr-gateway`, by name, all pass): `a_lagged_guild_topic_resends_the_guild_to_its_sessions`, `a_lagged_guild_topic_resend_is_filtered_per_member`, `a_resync_resends_the_thread_list_before_the_marker`, `a_resync_is_not_sent_to_a_member_who_left_meanwhile`, `a_second_lag_inside_the_floor_waits_and_then_runs_once`, `a_snapshot_overtaken_by_a_structural_event_is_rebuilt`, `bot_sessions_are_not_resynced`, `a_lagged_platform_topic_sends_the_platform_marker` (tests.rs); `a_lagged_user_topic_sends_a_fresh_ready_and_guilds_with_the_same_session_id`, `a_resync_is_not_sent_to_a_revoked_session`, `resync_stays_under_the_session_rate` (paused time: 12 sessions at 4/s take 2 s), `the_queue_is_bounded`, `the_floor_counts_from_the_end_of_the_round` (resync_tests.rs); `a_resync_keeps_the_voice_state_of_the_session` (voice_tests.rs). The existing `a_lagged_guild_topic_refreshes_the_member_list` and `a_lagged_user_topic_rechecks_that_users_sessions` ran green unchanged. `cargo clippy -j 4 -p sr-gateway --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

**Watched fail without the change** (temporary env switches in the queue and the epoch bump, removed again): `a_lagged_guild_topic_resends_the_guild_to_its_sessions`, `a_lagged_guild_topic_resend_is_filtered_per_member` (no resync requested), `a_snapshot_overtaken_by_a_structural_event_is_rebuilt` (no epoch bump).

Web (vitest, single files, all pass): new `store/resync.test.ts` (8: stale marking keeps pending sends and other guilds, typing, ignored markers, member refetch as a replacement, no fetch when never loaded, plain fetch still merges, second READY keeps pending and replaces read state, READY removes a gone server); `gateway/gateway.test.ts` (second READY keeps the sequence counter); `lounge/__tests__/threads.test.tsx` (2 prune tests); `announcements.test.tsx` and `moderation/__tests__/reports.test.tsx` (platform marker refetches, guild marker does not). Also run green: `store.test.tsx`, `connect.test.ts`, `readyManyGuilds.test.ts`, `developers/__tests__/gatewayDocs.test.ts`, and nine other files that use `'READY'` (163 tests, one lock round). tsc, eslint and prettier clean on my files.

## Proposed inventory row (SEC-AUD-169)

SEC-AUD-169 | gateway | **Lost gateway dispatches after bus lag (SEC-AUD-161 design)**: a guild or user pump that lagged re-sent nothing, so clients kept a deleted channel, an old permission or a stale unread mark until reconnect. Fixed (D-646): after the D-628 re-checks, the already-connected non-bot sessions get fresh state on the same socket and session (per-member `GUILD_CREATE` + `SR_RESYNC` for a server, `READY` with the same `session_id` + guilds for a user, the platform marker), through a queue with a per-key floor (30 s, platform 60 s), a bound of 256 keys and a 40-sessions-per-second cap; a snapshot overtaken by a structural event is rebuilt. Web: stale windows, member refetch as a replacement, flags and notices refetch, gone servers and threads pruned. Bots: docs say REST is the source of truth. | tests: the sr-gateway tests above, `store/resync.test.ts`.

## Proposed threat-model sentence

A resync snapshot is built per member with the same visibility rules as `GUILD_CREATE` at connect and only after the credential re-check was queued on the socket, so recovering from lag never shows a member more than a fresh connection would, and a revoked or departed member receives nothing; the cost of a lag storm is bounded by the per-key floor, the 256-key queue bound and the node-wide 40-sessions-per-second cap (residual risk: a flood that keeps the guild topic lagging still costs one bounded round per 30 s per server, and bot sessions stay exposed to lost events by design).

## Changelog sentence

When the server is under heavy load and drops live updates, your open client is now brought back up to date in place (channels, permissions, unread marks, announcements) without reconnecting or interrupting a voice call.

## Owner questions (for a Fable agent; defaults taken)

1. READY's `user` after a user-topic resync is the session's stored user object (at most one credential re-check old). Default: accept. Alternative: have the connection build the READY after its own re-check.
2. READY now removes every `guilds` entry it does not list. If the web ever keeps a guild the member is not in (a preview), it is dropped at the next reconnect. Default: accept (none found in `apps/web/src`).
3. The bot paragraph lives in the developer docs "Gateway" section (`docsContent.ts`); the protocol doc has the full text.

## Cleanup

Cargo target `p64-resync-target` deleted; no scratch database was created; scratch logs and the run script were in `SIDEROOM_wt` (outside the repo) and removed. No stack was started. Heavy lock released after each run.
