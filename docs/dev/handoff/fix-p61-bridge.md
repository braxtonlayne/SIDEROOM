# Wave 61: bridge (SEC-AUD-129..133, D-623, D-626, D-627)

Branch `fix/p61-bridge`, base `5e5108fd`. No migration. One response shape change (OpenAPI and client regenerated, MSW handlers regenerated). UI change on Server Settings > Discord bridge.

## Done (commits)
- `f43525ab` SEC-AUD-129..133 (Rust, web, OpenAPI, client, tests).
- Follow-up commit(s) on this branch: shots spec, phone fix for the warning row, this handoff.

## Items
- **129 (D-623)** `status.rs`: `MapView.shared_on_instance` (one query for the guild's maps, `EXISTS` on `discord_channel_id` with another map id). API `MapObject.shared_on_instance` (create answer sets `false`). Web: one `Notice` (warning) in its own full-width table row under the flagged map, no button; on a phone the table scrolls sideways, so the line is sticky and as wide as the visible area (`container-type` on the table, `100cqw`), otherwise it was cut off. The other server is never named or identified. Tests `a_discord_channel_shared_by_two_servers_is_flagged_on_both`, `a_map_nobody_else_holds_is_not_flagged`; web test in `bridge.test.tsx`.
- **130 (D-626)** `outbound.rs`: `run_outbox_once` re-arms dead erasure deletes once `erasure_rearm` (new `Tuning` field, default 24 h) has passed since `due_at`; a re-armed row that fails again goes straight back to `dead` (re-arm sets `due_at = now()`, so it waits another interval). An erasure row of a map with no webhook is killed ("no webhook to delete with"), other rows are still dropped. `StatusReport.erasure_leftovers` (Discord channel id and message id only) and `erasure_leftovers` in the API (strings). Web: a new "Messages to delete on Discord" card (only when the list is not empty) with the notice text of the task and one line per entry (channel name from the maps, else the id; the message id). There was no existing leftovers block on the screen (the webhook leftovers are only in the teardown flow), so the card is new and sits above "Switch off".
- **131 (D-627)** Outbound `attempt`: the map insert is tried three times (0, 100, 400 ms); if it still fails the posted copy is deleted (warn with both ids if that fails) and the error returns as before. Inbound `handler.rs`: on an `insert_inbound_map` error the stored message goes through `sink.delete` (warn on failure) and the error returns. Tests `a_map_insert_failure_after_the_post_leaves_one_discord_copy` (a trigger that raises, dropped before the retry), `inbound.rs::a_failed_inbound_mapping_removes_the_stored_message`.
- **132** `structure.rs::is_echo(data, bot_user_id)`: prefix AND `user_id == bot_user_id`. Tests `structure.rs::a_member_writing_the_bridge_reason_is_still_attributed` (unit) and an added step in `inbound.rs::structural_dispatches_reach_the_reconciler_and_the_bridges_own_echoes_do_not` (a member with the prefix is attributed).
- **133** Test only: `a_map_created_by_another_server_during_the_webhook_call_is_refused` passes at once (documents the behaviour, no code change). To hold the webhook creation on the wire I added `create_gate` to the fake Discord in `tests/support/mod.rs` (same `Gate` type).

## Tests run
- `cargo clippy -j 4 -p sr-bridge -p sr-api --all-targets -D warnings`: clean (first round).
- Passed: all 11 new/related `db.rs` tests (incl. `a_dead_erasure_delete_that_fails_again_waits_another_interval`, extra), `inbound.rs` tests above, the two `structure.rs` unit tests. First run had one failure in my own test (`a_dead_ordinary_post_is_not_rearmed` counted failed POST attempts as posts); fixed and green.
- Watched fail without the change (outbound.rs and handler.rs put back to HEAD, one lock round): `a_dead_erasure_delete_is_retried_after_the_rearm_interval`, `a_map_insert_failure_after_the_post_leaves_one_discord_copy`, `an_erasure_delete_without_a_webhook_is_listed_not_dropped`. Not observed failing: `a_failed_inbound_mapping_removes_the_stored_message` (cargo stops at the first failing test binary, so the `inbound` binary did not run in that round), 129 and `dead_erasure_deletes_are_listed...` (they do not compile without the new fields, which is a failure of a sort), 132 (the old `is_echo(&Value)` has no bot argument; the unit test asserts what the old code returned true for).
- Web: `bridge.test.tsx` and `sync.test.tsx` (37 tests) pass; typecheck, eslint and prettier clean on the changed files. `mocks:check` was out of date after the OpenAPI change and was regenerated.
- Not run: whole suites, route matrix (no route added), fmt check (rustfmt run on my files).

## Visual check
Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p61-bridge/` : `bridge-maps-*`, `bridge-erasure-*`, `bridge-clean-*`, each for desktop and phone in `lounge-editorial`, `lounge-light`, `lounge-dark` (18 PNGs), all looked at. `-maps-` shows the flagged row with its warning under the map and the unflagged row without; `-erasure-` the new card; `-clean-` the same screen with nothing to report (unchanged, no warning, no card). The first phone shots showed the warning cut off by the sideways-scrolling table; fixed and re-shot. The first dark shots were light because the spec did not set the account theme; fixed in the spec (account setting plus colour scheme) and re-shot. `settings-server-bridge` in the ordinary sweep (`sweep/` subfolder, first run) shows the Overview section, not the bridge, because the sweep world has the module off (the entry is hidden and the router falls back); it is unaffected by this change and cannot show the bridge. Spec: `e2e/tests/p61-bridge-shots.spec.ts` (`SR_P61_BRIDGE=1`, fulfils the features list, the status and the discord-channels requests).

## Proposed inventory rows (after SEC-AUD-128)
- [x] **SEC-AUD-129** [BE][FE] The bridge screen says on a map's row when another server on this instance also bridges its Discord channel (`shared_on_instance`); the other server is never identified (D-623).
- [x] **SEC-AUD-130** [BE][FE] A Discord deletion owed to an account erasure is never given up: dead rows are retried daily, one with no webhook is dead instead of dropped, and `erasure_leftovers` (Discord channel and message id, never the author) are listed on the bridge screen (D-626).
- [x] **SEC-AUD-131** [BE] A bridged message is stored with its mapping or not at all: outbound retries the map insert twice and otherwise deletes the posted copy; inbound removes the stored message when its mapping fails (D-627).
- [x] **SEC-AUD-132** [BE] An audit-log entry is the bridge's echo only when its reason has the `sr-bridge:` prefix and it was made by the bridge's own bot.
- [x] **SEC-AUD-133** [BE] The late refusal of D-609 (inside the insert transaction) has a test.

## Proposed threat-model rows (T33)
- D-609 residual: "existing duplicate maps are not removed, and each owner is told on the bridge screen" (`db.rs::a_discord_channel_shared_by_two_servers_is_flagged_on_both`). Late race covered by `a_map_created_by_another_server_during_the_webhook_call_is_refused`.
- Erasure sentence extended (D-626): Discord copies of an erased member's messages are deleted by the queue, retried daily when Discord fails, and listed for the owner by channel and message id (`dead_erasure_deletes_are_listed_for_the_owner_without_the_author`, `a_dead_erasure_delete_is_retried_after_the_rearm_interval`). Residual: an erasure deletion goes with its channel map when the map is deleted (FK cascade), and the webhook that could delete it goes too.
- Echo filter: a Discord member cannot hide their name from the conflict row by writing the bridge's reason.

## Owner questions
- The erasure list shows to everyone who can open the bridge screen (MANAGE_GUILD), like the maps, not to the owner only. Wanted?
- A phone shows the warning pinned to the visible width while the table scrolls; a stacked row layout for the whole table on phones is a possible later change.

## Flags outside my files
None (`bridge_dsr.rs`, `sr-server/src/bridge_*.rs` untouched). Scratch DB `sr_p61_bridge` is left for the commander to drop; `sr_p61_bridge_ui` is dropped. Untracked scratch scripts/logs (`p61-*.sh`, `p61-*.log`, `p61-ui-bin/`) are in the worktree, not committed.
