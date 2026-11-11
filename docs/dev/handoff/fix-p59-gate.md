# Handoff: wave 59 gate (SEC-AUD-066..072, D-599..D-603)

Branch `fix/p59-gate` (base main `9d83ee2c`). No UI, no migration, no new route.

## Done

| ID | decision | change | test (fails on main) |
|---|---|---|---|
| SEC-AUD-066 | D-600 | `PresenceUpdate::visible_activities()` (`protocol.rs`) returns `[]` when the visible status is `offline`. Used by `friend_presence_body`, `presence_body` and the member-list item. The `GUILD_CREATE` and op 8 presences already skipped offline users, so they never carried activities. | `tests.rs` `an_invisible_members_presence_carries_no_activities` (PRESENCE_UPDATE, friend presence, member-list item) |
| SEC-AUD-067 | D-601 | `guild_create_body` and op 8 (`presences_of`, now async) call `dir.blocked_peers(viewer)` once and leave those users out. Member-list lists are shared per channel, so `member_list::strip_blocked_presence` removes `presence` from the viewer's own `GUILD_MEMBER_LIST_UPDATE` body at send time (first sync in `lazy_request`, refreshes in `refresh_member_lists`, blocked set read once per subscriber per refresh). | `guild_create_presences_omit_a_blocked_member`, `member_list_item_has_no_presence_for_a_blocked_viewer` (also checks an unblocked viewer still sees it and that live updates stay stripped), `members_request_tests.rs` `member_request_presences_omit_a_blocked_member`. Test directories got `blocked_peers` overrides (`FakeDir.blocks`, `TestDirectory` + `TestWorld::block`). |
| SEC-AUD-068 | D-199 | `Dispatch.age_channel` (new field); `THREAD_DELETE` sets it from `payload["parent_id"]`; `dispatch_guild_gated` computes the 18+ gate from `channel.or(age_channel)` (`dispatch_guild` stays as a wrapper). | `thread_delete_under_an_age_restricted_parent_skips_unconfirmed_sessions` |
| SEC-AUD-069 | D-599 | `moderate_voice`: a move into an `age_restricted` channel of a target without `age_confirmed`, or of an `account_restricted` target, returns new `ModError::TargetCannotJoin` before any state changes. Mover rights unchanged. `commit_state_change` now logs a failed `voice_server_update` at `warn`. | `voice_tests.rs` `a_moderator_cannot_move_an_unconfirmed_member_into_an_age_restricted_channel` (includes the still-allowed confirmed move and an unconfirmed move between ordinary rooms), `a_moderator_cannot_move_an_account_restricted_member` |
| SEC-AUD-070 | D-603 | `stage_open_for_event` calls `stage_target(started_by, channel)` and needs `is_stage_moderator`; otherwise `MissingPermissions`, nothing opened. | `voice_ext_tests.rs` existing test now uses a moderator (renamed `..._for_a_moderator_creator`), new `stage_open_for_event_refuses_a_non_moderator_starter` |
| SEC-AUD-071 | - | `gateway_pg.rs` `member_list` and `activities.rs` `viewers` use `overwrite_source` + `needs_thread_membership` + `permissions_with_overwrites` like `dispatch_audience`. A thread whose parent is gone yields no list / no viewers; `member_list` also returns `None` for a channel that is not in the guild. | Postgres `#[ignore]` `member_list_of_a_thread_follows_its_private_parent` (public thread under a private parent, private thread with and without membership, a joined member who cannot see the parent stays out). No new test for `activities.rs::viewers` (same rule, same helpers). |
| SEC-AUD-072 | D-602 gateway half | `GUILD_SCHEDULED_EVENT_*` dispatches use `payload["channel_id"]` (when a string) as the dispatch channel. | `scheduled_event_in_a_hidden_channel_is_not_dispatched_to_non_viewers` (hidden channel, external event with `channel_id: null`, visible channel) |

`scheduled_events.rs` `dispatch_value` does include `channel_id` (`ScheduledEventObject.channel_id: Option<String>`), so no sr-api change was needed for that.

## Touched outside my file list (flag for the integrator)
- `crates/sr-server/src/voice.rs`: one line in `map_err` for the new `ModError::TargetCannotJoin` (the match is exhaustive).
- `crates/sr-api/src/routes/voice.rs`: new `VoiceError::TargetCannotJoin`, mapped to `SrConflict` (409, code 900010) with the message "That member cannot join this channel: it is age-restricted and they have not confirmed their age, or their account is restricted." Additive, 6 lines. No OpenAPI change (VoiceError is not a schema).
No sr-chat change.

## Findings the commander should know
1. **D-603 is only half true on main.** The plan says the scheduler already treats a failed stage open as non-fatal. It does not: `sr-chat/src/settings/service/event_ops.rs` `open_stage` is used in two places. A manual start (`:145`) returns the error, so a non-moderator creator now cannot start a stage event by hand; the voice-join start (`:303`) `continue`s, so the event then stays `scheduled` and never starts (the comment there says "a stage is already running"). D-603 says the event should still start without a stage. That needs a change in sr-chat (treat `MissingPermissions` from `start_stage` as "start without a stage"), which is the chat builder's crate. `events_jobs.rs` `start_stage` maps `MissingPermissions` to the generic "The stage could not be started." text.
2. `GUILD_SCHEDULED_EVENT_USER_ADD/REMOVE` payloads have no `channel_id` (`{guild_scheduled_event_id, user_id, guild_id}`), so they still go guild-wide (they carry ids only). To close it the API half would need to add `channel_id` to those payloads.
3. If the event's channel is already deleted when `GUILD_SCHEDULED_EVENT_DELETE` is emitted, `dispatch_audience` answers empty for an unknown channel, so nobody gets the delete. Probably acceptable (the channel delete cascades the events on the client), flagged anyway.
4. A blocked member still occupies a row in the shared member list (counts, group); only the `presence` field is stripped for the viewer, so the client shows that row as offline-ish.

## Not done
- Inventory rows (`docs/inventory/21-feature-phase.md`): not added, to avoid four builders conflicting on the same lines; the plan says the integrator adds them. Proposed text: SEC-AUD-066..072 = the table above, one row each, handoff `fix-p59-gate.md`.
- Threat-model rows: proposal only (the commander numbers them). D-600/D-601: "Presence side channel. Asset: whether a member is online and what they play. Threat: an invisible or blocked member's presence is inferred from activities, the initial guild payload, the member list or an op 8 request. Mitigation: one `visible_activities` helper and one per-build `blocked_peers` filter on every presence path; tests per path."
- No separate test for `activities.rs::viewers`.

## Tests run
See the bottom of this file (filled in after the gate run).

## Owner questions
- D-599: the error text for a refused move is shared between the 18+ and the account-restriction case. Should they say different things (the mover does not need to learn why a stranger's account is restricted, so one generic text was chosen on purpose)?
