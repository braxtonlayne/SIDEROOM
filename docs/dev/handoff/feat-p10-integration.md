# Handoff: phase 10 integration (`integration/phase10`)

Merged, in order: `feat/p10-lounge-be` (already in), `feat/p10-sset-be`, `feat/p10-voice-be`, `feat/p10-admin-be`, `feat/p10-lounge-fe`, `feat/p10-sset-fe`, `feat/p10-voice-fe`, `feat/p10-admin-fe`. Nothing pushed.

## Decision: one server tag implementation

lounge-be (LNG-GLD-011) and sset-be (LNG-SSET-011/012) both built server tags and tag adoption.

- **Kept: sset-be's.** `guild_tags` (tag, badge id 0 to 15, colour, `members_can_display`) and `user_primary_guilds` (migration 0631), `GET/PUT/DELETE /guilds/{id}/tag`, `GET/PUT /users/@me/primary-guild` `{guild_id}`. It is the richer model (colour, the "Members can choose to display" toggle, adoption cleared when the tag is deleted, audited changes).
- **Removed from lounge-be:** `guilds.tag`, `guilds.tag_badge`, `accounts.primary_guild_id`, `accounts.primary_guild_enabled` (migration 0620 is unreleased, so it was edited in place; every other 0620 change is untouched: vanity shape and uses, temporary members, screening, directory), `GET /guilds/tag-badges`, `PATCH /users/@me/primary-guild`, `tag`/`tag_badge` on the guild object and on `PATCH /guilds/{id}`, `sr_chat::server_tag`, the `set_primary_guild` repository and service methods, and their tests.
- **Kept from lounge-be, now reading sset-be's tables:** `primary_guild` on member, ban and other `UserRef`s. It is the sset shape `{identity_guild_id, identity_enabled, tag, badge, color}`, filled by `PgChatRepo::users` through the `guild_tags` join (shown only while the tag exists, members may display it, and the person is still a member).
- One primary-guild route remains: `GET/PUT /users/@me/primary-guild`. There is no PATCH.
- No badge list endpoint: badges are the fixed ids 0 to 15 that the client maps to glyphs.
- Tests: `server_tag_is_shown_next_to_members_who_opt_in` (sr-api `tests/lounge.rs`, rewritten against the kept API; it also checks that the removed routes and fields are gone) and `server_tag_adoption_shows_only_for_members_while_allowed` (sr-db). lounge-be's `server_tag_shows_only_for_members_who_chose_it` was dropped because it duplicated the second one.
- Docs: `docs/04` (tables), `docs/05` (server tag paragraph), inventory LNG-GLD-011 note, and a "superseded" note in `feat-p10-lounge-be.md`.

## Other conflict resolutions

- Every other code conflict was "both sides". Examples: `MessagesDeps` has both the age gate and the audit sink; the settings router has follow, tags, insights, guide, applications and soundboard; the gateway has the lazy list (op 102) and the stream and soundboard ops; `PgGuildDirectory` has temporary members and voice entitlements; the in-memory invite join has lounge-be's vanity path plus admin-be's Staff Server refusal.
- `docs/05`: the voice wire summary is now §6.8, after §6.6 lazy member list and §6.7 temporary membership.
- Inventory: tick union, so a box ticked on either side stays ticked. Where both sides left a box unticked, the line with the longer progress note was kept. LNG-CHN-021 now says the follow backend exists.
- Generated files (OpenAPI, client, MSW mocks) were regenerated after each backend merge. Route matrix: the union of both sides, minus the two removed routes, then `SR_UPDATE_ROUTE_MATRIX=1`. There are no `?` cells.

## Checks

- Migrations are unique and ordered: 0620, 0630 to 0634, 0640.
- Job ids are unique: 1 to 4, 110, 120, 130 to 132, 141, 150, 160 to 162, 172, 540, 541, 630. voice-be adds none.
- No route is registered twice. The router builds in the route-matrix test.
- `AuditSink::record_with_options` has one definition, sset-be's, which lounge-be did not touch.

## Fixes made during integration

- **Overwrite audit:** `set_overwrite` logs `CHANNEL_OVERWRITE_CREATE` (13) for a new overwrite, `UPDATE` (14) for a changed one and `DELETE` (15) when an empty allow and deny remove the row. Test: `overwrite_audit_tells_create_update_and_clear_apart`.
- **`is_staff_guild` (admin-fe contract):** sent as `true` on the Staff Server in the guild object (`GET /guilds/{id}`, and `GUILD_CREATE`, which follows READY) and in `GET /users/@me/guilds`. Other servers omit it. Test: `staff_server_is_flagged_in_guild_payloads`.
- **Voice contract (voice-fe against voice-be):**
  - `CALL_CREATE` and `CALL_UPDATE` now also carry `initiator_id` and `group`.
  - The client now accepts the gateway's `SR_VOICE_ERROR` codes `video_limit_reached` and `stream_full`. Before, it only knew its proposed names `video_limit` and `viewer_cap`.
  - Stream-op errors arrive with `channel_id: null`. The client used to drop them as "another channel", and would otherwise have ended the call.
  - Op 13 `QUALITY` (`{reduced, max_layer, reason}`) already matched the client's `{reduced}`.

- **Soft delete versus lounge-be data:** sset-be made guild deletion a soft delete (0630), so lounge-be's directory listing (which went with the row through `ON DELETE CASCADE`) stayed, and the vanity code stayed claimed and stayed resolvable by vanity lookups that do not filter deleted servers. `delete_guild` now removes the `guild_directory` row and clears `vanity_code`. This was caught by sr-db `directory_listings_round_trip_and_filter`, which now also checks that the vanity code is released.

## Open

- LNG-SSET-011 (tag editor UI) and the user-side tag picker are not built. The API is ready.
- admin-fe still has no "SIDE ROOM Staff" profile badge.
- `CALL_CREATE` does not say whether the caller started with video, so the client shows a voice ring.
