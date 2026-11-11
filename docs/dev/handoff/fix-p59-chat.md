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
See the final summary in the reply (cargo fmt, clippy, lib suites; Postgres test result noted there).

## Routes shot
None (no UI change).

## Owner questions
- 073 answers 503 with the generic overloaded error text; a dedicated message ("try again in a moment") could be added to `sr-core` if wanted.
- 077: a user with MANAGE_EVENTS but no VIEW_CHANNEL also no longer sees the event (Discord parity); say if staff should see it.

## Threat-model rows proposed
- Posting restriction lookup fails closed (D-598). Hidden-channel scheduled events (D-602). Ban beats pending application (D-604). Webhook move needs destination rights (D-607).
