# Wave 59 handoff: chat (SEC-AUD-073..079, D-598, D-602 API half, D-604, D-607)

Branch `fix/p59-chat` on main `9d83ee2c`. No UI change, no migration, no new route, so the OpenAPI file is unchanged (the new answers are an existing error code on existing routes; 503 on posting routes and `UserBanned` on application decision are not declared per route in the spec).

## Done
- 073: trait returns `Result<bool, String>`; `ensure_may_post` answers 503 (`ApiResourceOverloaded`, retryable) and logs. Pg impl no longer `unwrap_or(false)`. Test `a_failed_restriction_lookup_does_not_let_a_send_through`.
- 074: `extract_hosts` trims trailing dots. Test `a_trailing_dot_host_still_matches_the_blocklist`.
- 075: ban purge adds `list_guild_threads`. Test `ban_purge_hides_messages_in_threads`.
- 076: `modify_inner` runs `channel_access` (VIEW_CHANNEL, 403 like create) and requires MANAGE_WEBHOOKS in the destination. Tests `a_webhook_cannot_be_moved_into_a_channel_the_manager_cannot_see`, `a_webhook_move_needs_manage_webhooks_in_the_destination`.
- 077: list drops, get / RSVP (put and delete) / interested-users list answer 404 `UnknownScheduledEvent` for an event whose channel the caller cannot view. External events and events whose channel is gone stay guild-wide. `dispatch_value` already carries `channel_id` (unchanged). Test `events_in_a_hidden_voice_channel_are_not_listed_got_or_rsvpable` (in `settings/tests.rs`).
- 078: service refuses approval of a banned applicant (`SettingsError::Banned`, 403 `UserBanned`); Pg `decide_application` re-checks `guild_bans` inside the tx and rolls back with `APPLICANT_BANNED`; Pg `insert_ban` (`sr-db/src/guild_admin_repo.rs`, outside the listed files, but it is the repo-side ban transaction) rejects the user's pending applications in the same tx; memory mirror (`MemorySettingsRepo::place_ban`, check in `decide_application`). Tests: sr-chat `a_ban_rejects_the_pending_application_and_refuses_its_approval`; Postgres `#[ignore]` `approving_an_application_of_a_since_banned_user_adds_no_member` (also covers the ban rejecting).
- 079: `parent_overwrites` helper, `?` on error. Test `create_channel_fails_when_the_parent_cannot_be_read`: `GuildBackend` is too large to stub, so the parent read is injected as a closure and the helper is tested (the creation call follows only after `?`).

## Not done / notes
- A narrow race remains in 078: a ban transaction that is uncommitted while an approval runs is not seen by the approval. Closing it needs a row lock on the ban path; left.
- Only `ban_member_purging` paths through `insert_ban` reject applications; the memory chat repo's `insert_ban` cannot reach the settings memory store (test mirror is `place_ban`).

## Tests run
- `cargo fmt --all -- --check` clean. `cargo clippy -p sr-chat -p sr-api -p sr-db -p sr-server --all-targets -- -D warnings` clean.
- `cargo test -p sr-chat --lib` 395 passed; `-p sr-api --lib` 849 passed (18 ignored); `-p sr-server --lib bridge_sync` 1 passed.
- Postgres (`sr_p59_chat`, dropped after): `cargo test -p sr-db --test settings_repo -- --ignored`: 10 passed, 3 failed: `emoji_rules_templates_and_community_round_trip`, `an_audit_purge_that_removed_rows_is_itself_audited`, `insights_are_computed_per_day_and_read_back`. None touches this work; the tests in that file share snowflake worker ids across concurrent tests and the purge test deletes audit rows other tests write, so they look like parallel-run interference (not re-run alone or on main: not verified). The new test `approving_an_application_of_a_since_banned_user_adds_no_member` and the existing applications test pass.
- Not run: sr-db/sr-server integration suites beyond the above, `authz_matrix` (CI runs them).

## Routes shot
None (no UI change).

## Owner questions
- 073 answers 503 with the generic overloaded error text; a dedicated message ("try again in a moment") could be added to `sr-core` if wanted.
- 077: a user with MANAGE_EVENTS but no VIEW_CHANNEL also no longer sees the event (Discord parity); say if staff should see it.

## Threat-model rows proposed
- Posting restriction lookup fails closed (D-598). Hidden-channel scheduled events (D-602). Ban beats pending application (D-604). Webhook move needs destination rights (D-607).

## Follow-up: SEC-AUD-070 (D-603), stage open is non-fatal at event start
`event_ops.rs` `open_stage` no longer returns an error: a refused stage open (busy, or, with the gate branch, the creator lacks stage-moderator rights) is logged and the event still goes `active` with no `entity_id` (no stage). Both paths: manual start in `update_event` and `start_on_voice_join` (which used to leave the event `scheduled` forever). Tests (stub world that refuses the open): `a_refused_stage_open_does_not_stop_a_manual_start`, `a_refused_stage_open_does_not_stop_a_voice_join_start`. The old "a busy stage blocks the start" assertion was removed from `a_stage_event_opens_and_closes_its_stage_instance`, since a busy stage now also starts the event without a stage (same rule, D-603). `sr-chat --lib` 397 passed, clippy clean. The `GUILD_SCHEDULED_EVENT_STAGE_BUSY` 400 is gone; no OpenAPI change.
