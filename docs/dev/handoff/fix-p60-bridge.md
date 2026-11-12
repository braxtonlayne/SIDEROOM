# Wave 60: bridge (SEC-AUD-099..108, D-609..D-611)

Branch `fix/p60-bridge`, base `34912d1a`. One migration (`2610_bridge_discord_channel_index.sql`, a plain index). No routes, no UI, no new dependencies.

## Done (commits)
- `e84635d5` SEC-AUD-099, 105, 107, 108.
- `e79cc366` SEC-AUD-100, 101, 102, 104, 106 (+ migration 2610).
- `10f270e2` SEC-AUD-103.

## Items
- **099** `reconcile_run.rs`: `sr_ch.get(&c.id)` else `continue` (the next pass maps it). The other `[&..]` in `engine.rs` is guarded by `contains_key` on the same snapshot. Test `sync_engine.rs::a_channel_created_during_a_reconcile_pass_does_not_panic` (in `sync_engine.rs`, not `db.rs`: the reconcile pass needs that file's guild fake; the SIDE ROOM port creates a channel inside `apply`, between the two reads).
- **100 (D-610)** `outbound.rs::send_row`: a `webhook_of` error goes to `retry_or_kill` (backoff, counted attempt, dead letter with the error text); `Ok(None)` still drops. Tests `a_webhook_lookup_error_keeps_the_queued_post`, `an_erasure_delete_survives_a_webhook_lookup_error`.
- **101 (D-610)** `outbound.rs::finish_create`: after a successful create post the worker removes its row with `DELETE .. RETURNING payload` in a transaction. No row back: queue a `delete`. Different payload: queue an `edit`. Skipped (debug log) when the map is gone (an `INSERT .. WHERE EXISTS` instead of catching an FK error, because an FK error would abort the transaction). No migration. Tests `a_delete_while_the_post_is_in_flight_deletes_the_discord_copy`, `an_edit_while_the_post_is_in_flight_is_applied_after_the_post` (a new `Gate` in `tests/support/mod.rs` holds a webhook execution on the wire).
- **102 (D-609)** `maps.rs::create_map`: `refuse_taken` refuses a Discord channel that any map on the instance holds ("That Discord channel is already bridged on this instance."), checked before the webhook is created and again inside the insert transaction under `pg_advisory_xact_lock(discord_channel)`; a late refusal deletes the just-created webhook (the existing `discard_webhook`). Inbound `is_own_webhook` no longer filters by guild (its signature lost the `guild` argument). Migration 2610: plain index on `bridge_channel_maps (discord_channel_id)`. Tests `a_discord_channel_mapped_by_another_server_is_refused`, `inbound.rs::a_post_from_another_servers_bridge_webhook_is_not_imported`. The late-race refusal itself has no test (it needs two concurrent creates).
- **103** `sr-server/src/bridge_inbound.rs`: the message store and the rich store are different traits, so no single transaction. I did the other option: `insert_with_author` soft-deletes the just-inserted message (UserDeleted, purge due at once) when the author link fails, then returns the error. Tests (unit, memory repo) `a_failed_author_link_leaves_no_bridged_message`, `a_stored_author_link_keeps_the_message_visible`.
- **104** `status.rs::erase_for`: the map row goes only after `Ok` or `Gone`; a refusal or an unusable webhook keeps it (not counted as removed). Method kept (still used by `tests/db.rs`). Test `erase_for_keeps_the_mapping_when_discord_refuses_the_delete`.
- **105** `sync/teardown_opts.rs`: the six list reads use `?` (the overwrite JSON parse default is kept, it is not a read); `teardown_discord_structure` returns `BridgeResult`. I also moved that call in `teardown.rs` BEFORE the webhook deletions, so a failed read stops the teardown before anything is deleted anywhere. Test `sync_engine.rs::a_teardown_whose_list_query_fails_deletes_nothing` (renames a column in the test's own database).
- **106 (D-611)** `tokens.rs`: an existing bridge refuses (Conflict, "This bridge is connected to another Discord server. Disconnect it first to change servers.") when the chosen server differs; every replacement writes an audit row `AUDIT_INTEGRATION_UPDATE = 81` (`service.rs`) in the same transaction, with no token in the detail. Tests `replacing_the_token_with_another_server_pick_is_refused`, `replacing_the_token_writes_an_audit_row`.
- **107** `sync/engine.rs` (`Unmapped`, `undo_unmapped`) and the 8 call sites: an ignored map insert deletes the object just created (Discord role or channel through the API; for the "created here" cases the SIDE ROOM one through the port's `delete_*`) and the pass returns a Conflict. Test `sync_engine.rs::a_role_map_conflict_removes_the_new_discord_role` (a hook in the guild fake adds the conflicting map row while the role is created). The other three shapes share the helper and are not tested separately.
- **108** `inbound/convert.rs`: `local_fake = cfg!(debug_assertions) && ..`. No test (CI does not build release); existing tests run in debug and stay valid.

## Tests run (owner instruction: only what proves the change, names filtered, short lock)
Clippy `-p sr-bridge -p sr-server --all-targets -D warnings` clean; `cargo fmt --check` clean. Passed: the new `db.rs` tests for 100/101/102/104/106 (first run: 8 of 10 passed, the two 100 tests failed on a bug in my own test helper, fixed and rerun green), the three new `sync_engine` tests, the new `inbound` test, the two `sr-server` `bridge_inbound` unit tests. The 108 line was edited after the last build (a `cfg!` on one boolean); CI compiles it.

**Not run against main:** I did not do the `git stash` round to watch tests fail on main (the owner asked mid-task to keep lock time short). Every test asserts behaviour the old code could not have (a panic, a dropped row, a stored duplicate, an unaudited replace, a deleted mapping), but none was observed failing.
Not run: the whole sr-bridge and sr-server suites, the authz matrix (CI).

## Threat model proposal (extends T33 for D-609)
Attack column: "one Discord channel mapped into two SIDE ROOM servers, so server B stores what server A's members said". Mitigation: "A Discord channel is bridged into at most one server per instance (`crates/sr-bridge/tests/db.rs::a_discord_channel_mapped_by_another_server_is_refused`); inbound ignores posts of any bridge webhook on the instance, so an install that already has a duplicate stops copying (`crates/sr-bridge/tests/inbound.rs::a_post_from_another_servers_bridge_webhook_is_not_imported`); a token replacement stays on its Discord server and is audited (`::replacing_the_token_with_another_server_pick_is_refused`, `::replacing_the_token_writes_an_audit_row`)". Residual: existing duplicate maps are not removed (no migration; the owner of either server can delete theirs); a post through a webhook that is not the bridge's own is still imported.

## Proposed inventory rows (after SEC-AUD-098)
- [x] **SEC-AUD-099** [BE] A reconcile pass skips a SIDE ROOM channel created between its two reads (the next pass maps it) instead of panicking on `sr_ch[&id]`.
- [x] **SEC-AUD-100** [BE] A webhook lookup error (database, unseal) reschedules the queued post with backoff and dead-letters it at `max_attempts` with the reason; only "no webhook" drops it (D-610).
- [x] **SEC-AUD-101** [BE] A delete or edit made while a bridged post is in flight reaches Discord: the worker removes its row with `DELETE .. RETURNING` and queues a `delete` (row gone) or an `edit` (payload changed) (D-610).
- [x] **SEC-AUD-102** [BE] One Discord channel is bridged into at most one SIDE ROOM server per instance (re-checked under an advisory lock in the insert transaction); inbound ignores posts of any bridge webhook on the instance. Migration 2610 adds a plain index (D-609).
- [x] **SEC-AUD-103** [BE] A bridged message whose author link could not be stored is removed again (deleted, purge due) before the error is returned.
- [x] **SEC-AUD-104** [BE] `erase_for` deletes a map row only after Discord confirmed the copy is gone.
- [x] **SEC-AUD-105** [BE] A failed list read during the whole-guild teardown stops the teardown before anything is deleted.
- [x] **SEC-AUD-106** [BE] Replacing the bot token keeps the bridge on its Discord server (another server: Conflict) and writes an INTEGRATION_UPDATE (81) audit row in the same transaction (D-611).
- [x] **SEC-AUD-107** [BE] An ignored map insert removes the object the pass just created (Discord or SIDE ROOM) and fails the pass with a Conflict.
- [x] **SEC-AUD-108** [BE] The localhost attachment exception of `is_discord_cdn` exists in debug builds only.

## Owner questions
- 102: existing duplicate maps stay; inbound only stops the cross-copy. Should a later wave list them for the owners?
- 107: the pass now fails with a Conflict instead of silently continuing; the next scheduled pass retries.

## Flags outside my files
None (`bridge_dsr.rs` and `bridge_sync_wiring.rs` untouched). The scratch DB `sr_p60_bridge` still exists; drop it at integration.
