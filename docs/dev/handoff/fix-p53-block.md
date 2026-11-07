# fix/p53-block (wave 53): blocks honoured everywhere

## Done
- **SEC-AUD-003** a block no longer leaves an existing 1:1 DM writable. New `DmSendGate` trait (`crates/sr-api/src/routes/messages/deps.rs`),
  implemented by `SocialDeps` with the already-written `SocialService::can_send_dm` (block in either direction), wired in
  `sr-server/src/app.rs`. `MessagesDeps::require_dm_open` answers Discord's 50007 "Cannot send messages to this user" and is called on
  message create (`send.rs`, which also covers forwards), adding a reaction (`reactions.rs`), poll vote (`polls.rs`) and scheduled
  send (`userfeat/scheduled.rs`). Editing or deleting your own old messages is not gated (Discord). Group DMs are never closed by a
  block. Call ring/join: `PgGuildDirectory::dm_call_members` (`sr-server/src/gateway_pg.rs`) now refuses a 1:1 DM with a block either way,
  so neither a join nor a ring starts (group calls unchanged).
- **SEC-AUD-011** mentions: `SocialService::mention_muted` + new repo method `muted_towards` (Postgres: one query) is used by
  `message_hooks.rs` (no mention badge for a user who blocked the author, was blocked by them, or ignores them) and by
  `sr-server/src/push_notify.rs` (no Web Push to them). Presence: gateway `GuildDirectory::blocked_peers` (Postgres impl in
  `gateway_pg.rs`); `hub.rs refresh_presence` sends the guild `PRESENCE_UPDATE` only to sessions not in a block with the user.
- **SEC-AUD-012** `sr-library-social`: comments and reviews skip authors in `user_ignores`; lists skip owners the viewer ignored and owners
  blocked in either direction; reading-presence audience checks blocks both ways and ignores (`audience_of`, and the viewer-side query).
  `sr-packages` profile page: top-friend tiles skip anyone the viewer has a block with, guestbook rows skip authors the viewer has a block
  with (the owner's own view is unchanged so they can still delete notes).

## Tests (fail without the fix)
- `sr-api` `routes::social::tests`: `a_block_closes_an_existing_dm_in_both_directions` (50007 both ways, own edit still works, reaction
  refused, works again after unblock), `a_block_never_closes_a_group_dm`, `a_blocked_users_mention_does_not_badge_the_blocker`.
- `sr-library-social` `tests/db.rs` (ignored, Postgres): ignore asserts added to the comments, reviews and featured-lists tests.
- `sr-packages` `tests/profile_privacy.rs` (ignored, Postgres): `a_viewers_blocked_people_are_left_out_of_top_friends_and_guestbook`.
- Tests run: cargo fmt --check clean; clippy -D warnings (sr-api, sr-chat, sr-gateway, sr-library-social, sr-packages, sr-db, sr-server, all targets) clean; sr-api social:: (57 pass), sr-chat (389), sr-gateway (206), sr-library-social db --ignored (19), sr-packages profile_privacy --ignored (5). Not run: web gates (no web change), sr-server PG tests.

## Not done / listed only
- Push to a blocked mention in a guild is filtered, but the member list / initial `GUILD_CREATE` presences still carry the blocker's
  presence for a blocked member (only live `PRESENCE_UPDATE` is filtered).
- "A sees B's messages collapsed in shared servers" (client-side collapse of a blocked author's guild messages) is a web feature; no
  API change was needed or made.
- The Cinema/other `relationships type IN (2, 6)` checks remain: type 6 is never written (ignores live in `user_ignores`); harmless.
- The call-ring guard is in the gateway directory (Postgres). Its unit test would need the PG voice test rig; the DM rule it shares is
  covered by the route tests above. Not separately tested.

## Owner questions
- Group DMs: Discord keeps a blocker and the blocked person in the same group DM and both can post. Kept. Do you want blocks to hide the
  blocked person's group messages for the blocker (client collapse) or leave as is?
- Mentions/push also suppressed when the MENTIONED user ignores the author (not only blocks). Kept as the sensible default.
- Guestbook: a blocked author's note is hidden from the blocking viewer but still shown to the owner (so it can be removed). OK?

## Routes shot
None: no UI text or CSS changed (server-side only), so no visual check applies.
