# Handoff: fix/p54-chat (wave 54 chat: hold release, posting gate, bridge limiter, channel overwrites, races)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p54-chat`, branch `fix/p54-chat` (base main `f581b538`). Not pushed.
No migration, no route added or removed, no OpenAPI change (the message edit handler gained an internal `State`
extractor only). No UI text or screen changed, so no visual sweep (see "Not done").

## Done

### SEC-AUD-023 (D-559): a hold release deletes an empty group DM
- `sr-moderation/src/holds.rs` `release_hold_in`: before clearing the rows, it collects the group DMs (type 3) the
  hold froze rows in (messages and attachments); after clearing, in the same transaction, it deletes those with no
  recipients and no other held message or attachment. Nobody can see such a group, so no gateway event. The case
  event `hold_released` carries `empty_groups_deleted`. `ReleaseOutcome` and the API response are unchanged.
- Test (Postgres): `sr-moderation/tests/db.rs` `releasing_the_last_hold_deletes_an_empty_group_dm`: two holds in a
  recipient-less group: the first release keeps it, the second deletes it with its messages; a group that still has
  a member, and the guild channel, stay.

### SEC-AUD-024 (D-566): the posting gate covers edits with new content, crosspost and poll votes
- `sr-api/src/auth/policy.rs` `posting_route`: adds `POST .../messages/{id}/crosspost` and
  `PUT .../polls/{id}/answers/{a}/voters/@me`. `DELETE` of a message (one's own) and of a vote stay open.
- A message edit needs its body: `Principal::ensure_may_post` (`auth/extract.rs`, the same check the extractor
  runs) is called by `edit_message` (`routes/messages/send.rs`) when the body carries `content`. A flags-only edit
  (suppress embeds) stays open. The edit body has no attachments field, so "new content" is `content`.
- Tests: unit `posting_routes_are_the_ways_to_post` (new routes, deletes and the PATCH not route-gated);
  `routes/messages/tests.rs` `the_posting_gate_covers_edits_crossposts_and_votes_but_not_deletes` (a browser session
  with a fake restriction gate: edit with content, crosspost, vote, new message -> 403 `900124`, content unchanged;
  flags-only edit 200; own delete 204); the HTTP gate proof `authz_matrix.rs` `graduated_moderation_holds_over_http`
  now also checks crosspost and vote for the frozen node.

### SEC-AUD-025 (D-567): bridge edits and reactions pass the per-bridge limiter; seed imports only confirmed bits
- `sr-bridge/src/inbound/handler.rs`: the limiter check is one helper `over_limit`, used by `create`, `update` (after
  the message is found to be a bridged one) and `reaction` (before the reactor is recorded), so a dropped reaction
  leaves no reactor row. Test (Postgres): `tests/inbound.rs` `edits_and_reactions_count_against_the_bridge_figure`.
- The unverified HIGH is **real**: `run_seed` re-reads Discord and imported `dr.permissions` (fresh) for a
  create-here role, while `confirm_mode` checked the Discord hash only at confirmation; a seed resumed after a restart
  re-reads hours later. Fix (`sync/engine.rs`): import `dr.permissions & item.discord_permissions` (the bits the
  stored, confirmed plan showed) within `BRIDGE_SHARED`; a bit added on Discord afterwards is never imported, a bit
  removed is not re-added. Test (Postgres): `tests/sync_engine.rs` `the_seed_imports_only_the_bits_the_owner_confirmed`
  (Manage Channels and Ban Members added to the Discord role between confirm and seed: only the confirmed bit lands).

### SEC-AUD-026 (D-570): channel edit and delete use the channel's overwrites
- `sr-chat/src/admin_service.rs`: `edit_channel` and `delete_channel` evaluate `MANAGE_CHANNELS` (and the move of the
  edited channel) with the actor's permissions in that channel (`access`, overwrites applied), as Discord does: a deny
  in the channel stops a guild-level holder, an allow lets a member in. Timed-out or pending members are read-only in
  channels, so they can no longer edit channels either (Discord parity).
- `sr-api/src/routes/guilds/channels.rs`: the 2FA gate of the same two routes evaluates the permission in the channel
  (`require_channel_perm(.., true)`), so it agrees with the service.
- Audit of the other `MANAGE_CHANNELS` checks for a specific channel: features/slowmode presets, feature scopes,
  invites, bridge held channels, voice rooms, jukebox, watch party, activities, timers all use channel-level
  permissions already. Create and bulk reorder stay guild-level (Discord).
- Test: `sr-chat/src/admin_tests.rs` `channel_edit_and_delete_follow_the_channels_overwrites`.

### SEC-AUD-027 (audit MED theme 3): races and full-row upserts
Data loss first:
- **Community settings revert a raid auto-pause** (real): `put_community` no longer writes
  `invites_disabled_until`, `dms_disabled_until` or `raid_detected_at` on an existing row (insert only); they change
  through `mark_raid`, `clear_invite_pause` and the new `CommunityRepository::set_incident_pauses`, which
  `set_incident_actions` now uses. Memory repo mirrors it. Test: `sr-db/tests/p14_j_lounge.rs` (a save from a read
  taken before the alert keeps the pause; the other tests set pauses through the new method).
- **Privacy settings written whole from a snapshot** (real): new `replace_privacy(user, expected, new)` (Postgres:
  the account row `FOR NO KEY UPDATE`, compare, write; memory: compare under the lock); `update_privacy` reads,
  applies the patch and retries (4 times, then `Conflict`) like the settings blob. Test: `sr-db/tests/social_repo.rs`
  (stale `expected` refused and nothing written; current one lands; no row yet = default).
- **Thread edit writes a stale snapshot** (real): `ThreadRepository::update_thread(before, after)` writes only the
  columns the edit changed, merges flag bits, and `last_activity_at` only moves forward (`GREATEST`). Test:
  `sr-db/tests/thread_repo.rs` `a_thread_edit_from_an_older_read_keeps_concurrent_changes`.
- **Read marker moves back / pin-seen time reverted** (real): `ReadStateRepository::apply_read_state(user, channel,
  ReadStateOp)` writes one change in one statement: `Ack` = `GREATEST` marker + mentions 0, pin untouched; `Pins` =
  `GREATEST` pin time only; `Unread` = marker + mentions (moving back on purpose). `ack`, `ack_pins`, `mark_unread` use
  it. Test: `sr-db/tests/social_repo.rs` `read_states_and_notifications` (an older ack against a newer stored marker,
  and an older pins ack, change nothing).
Caps:
- **Slowmode check-then-insert** (real): the last-sent read and the insert run under a striped per-(channel, author)
  async lock shared by every `ChatService` in the process (`sr-chat/src/service.rs`, tokio `sync` feature added to
  sr-chat's existing tokio dependency; no new crate). No test: the in-memory repository never yields, so a
  concurrency test cannot fail without the fix.
- **Per-reporter daily report limit** (real): `file_report` locks the reporter's account row before counting.
  Test (Postgres, multi-thread): `parallel_reports_do_not_pass_the_daily_limit_together` (4 parallel, limit 1 -> 1).
- **Cinema download allowance** (real): `DownloadRepository::create_download(row, since_ms, per_day) -> bool` counts
  and inserts in one transaction under the account row lock; the service maps `false` to the same 429.
- **Voice user limit and video allowance** (real): `sr-gateway` `voice_join` re-checks the occupancy (`check_join`)
  and the camera allowance under the state lock together with the insert (the counts were read before several
  awaits); `voice_stream` re-checks the allowance under its existing lock. The old channel's SFU disconnect is sent
  after the insert now (it was just before it). No new test (the existing voice tests cover the single-join paths).

## Not done
- Visual sweep: nothing visible changed (no web code touched).
- Not touched from theme 3 (other areas or other builders): moderation `execute_branch_action` double run,
  appeals ordering, app-install orphan role, `publish_now`, job-queue stalled worker, web-side races.
- `set_invites_paused` and `update_community` still save the other community columns from their read (two moderators
  saving at once: last writer wins, as before); only the raid and incident columns are protected now.

## Audit findings checked and found wrong
- None of the findings in this task were wrong. Every one above was confirmed in the code first (the bridge seed
  HIGH included). The test of SEC-AUD-023 is in the SEC-AUD-027 commit (same test file as the report-limit test).

## Tests run
- (fill in)

## Owner questions
1. Channel edit with a `MANAGE_CHANNELS` allow overwrite in one channel (no guild-level bit) now also lets that
   member move the channel (position, category). Discord lets the channel-level holder edit the channel; moving into
   another category is not separately checked. Default: allowed. Should a move need the bit in the target category?
2. Slowmode is serialised per process. With several server processes behind one database a burst split across
   processes could still pass twice. Default: per process (a database-side check would need the slowmode bypass
   decision passed into the message insert). Worth doing?
