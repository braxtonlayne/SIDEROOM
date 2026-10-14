# Handoff: feat/p10-lounge-be (phase 10, Lounge servers, channels, invites: backend) - COMPLETE

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p10-lounge-be, branch feat/p10-lounge-be (base integration/phase10).
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p10-lounge-be-target. Tests ran on a scratch DB `sr_p10_lounge_be`, dropped afterwards. Migration range 0620-0629: only `0620_lounge_servers.sql` is used.

Scope: docs/inventory/04-lounge-servers-channels.md, backend and gateway items LNG-GLD-005, 006, 008, 009, 010, 011, 012, 013, LNG-CHN-004, 005, 006, 007, LNG-INV-003, 004, 005. All fifteen are ticked.

## Audit: already built (ticked with a note, no new code)
- LNG-GLD-005: the voice part is `PATCH /guilds/{id}/voice-states/{user_id}` (mute, deafen, move, disconnect; `routes/voice.rs`). Everything else was done in phase 2.
- LNG-GLD-009 and LNG-CHN-007: `sr-gateway` `hub.rs` already dispatched every guild, member, role, ban, channel and pins event; `GUILD_CREATE` is lazy (members and presences only below the large threshold).
- LNG-CHN-005: slowmode was enforced in `message_rules::check_slowmode` with the bypass permissions.
- LNG-INV-003: the `external: true` flag on `POST /channels/{id}/invites` (not `target=external`) already makes the invite single use.

## Built
- **LNG-CHN-006 NSFW gate**: `MessagesDeps::access` (the one place every message endpoint resolves a channel) answers 403 `900011` for an NSFW channel, or a thread of one, until `accounts.age_confirmed_at` is set (D-032). The check runs after the permission check, so a member who cannot see the channel still gets 50001. `MessagesDeps::with_age_gate` is wired with `PgAgeGate` in `sr-server`.
- **LNG-GLD-006 bulk ban**: `POST /guilds/{id}/bulk-ban` (up to 200, `BAN_MEMBERS` + `MANAGE_GUILD`, per-user hierarchy, `{banned_users, failed_users}`).
- **LNG-GLD-011 server tag**: `guilds.tag/tag_badge` (set with `PATCH /guilds/{id}`, free), `GET /guilds/tag-badges`, `PATCH /users/@me/primary-guild`, `primary_guild` on every user reference while the person is still a member and the server still has a tag.
- **LNG-GLD-012 per-server profile**: `PATCH /guilds/{id}/members/@me` (nick, avatar, banner, bio), `GET /users/@me/guilds/{id}/member`; member objects carry `avatar`, `banner`, `bio`. New `MediaKind::MemberAvatar/MemberBanner` with a hash salted by the guild (`prepare_bytes_scoped`), so one picture in two servers is two objects and replacing one never deletes the other.
- **LNG-INV-004 vanity URL and splash**: `GET|PATCH /guilds/{id}/vanity-url`; `GET`/`POST /invites/{code}` fall back to the vanity code (never expires, no limit, counted in `guilds.vanity_uses`); the invite object's `guild` carries `splash`, `banner`, `description`, `vanity_url_code`. A real invite with the same text wins, and the PATCH refuses a code that equals an existing invite.
- **LNG-GLD-008 join pipeline**: the join transaction (`PgInviteJoinRepo`) inserts the member with `pending` from `guild_community.screening_enabled AND community_enabled`, `temporary` from the invite, counts vanity uses; `accept_invite` then posts the type 7 notice in the system channel (`messages/system.rs`). Screening form in `PATCH /guilds/{id}/sr/community` (`screening_enabled`, `screening_description`, `screening_rules`), readable by members at `GET /guilds/{id}/member-verification`; `PUT /guilds/{id}/requests/@me` accepts; pending members are read-only in `channel_permissions`. `POST /guilds/{id}/onboarding-responses` validates the answers (`validate_answers`), grants the roles of the picked options except managed roles and roles with moderation power, and returns `{channel_ids, role_ids}`.
- **LNG-INV-005 temporary members**: `guild_members.temporary`; `GuildDirectory::user_offline` fires once per online to offline transition; `TemporaryMembers` (sr-server) waits 60 s, checks the user did not come back, then `InviteMemberJoiner::drop_temporary` removes memberships without roles and emits `GUILD_MEMBER_REMOVE`.
- **LNG-GLD-013 Server Directory** (D-012): `guild_directory` table; `routes/directory.rs` (`DirectoryDeps`): categories (built-in list), search (text, category, featured first then by members, offset paging), single listing, `POST /directory/{id}/join` (same pause/ban/screening rules as an invite, session only), managers' `GET|PUT|DELETE /guilds/{id}/directory`, staff queue `GET /admin/directory` and `PATCH /admin/directory/{id}` (`MANAGE_DIRECTORY`, written to the platform access log). Editing an approved listing sends it back to review. Bots are denied on `/directory*` and on `PATCH /users/@me/primary-guild` (`auth/policy.rs`).
- **LNG-CHN-004 announcement channels**: `POST /channels/{id}/followers` (a follower webhook, kind 2, in the target channel), `POST /channels/{id}/messages/{id}/crosspost`, channel flag `AUTO_PUBLISH` (`1 << 24`, honoured by `create_message`), and `PATCH /channels/{id}` `type` 0 or 5 to convert text and announcement channels (this also unblocks the "announcement toggle" in LNG-CHN-017). Copies carry `IS_CROSSPOST`, the original author, text and embeds, a reference to the source and no pings; the source gets `CROSSPOSTED` and a `MESSAGE_UPDATE`. New error codes 40033 and 50024. New repo method `MessageRepository::set_message_flags`.
- **LNG-GLD-010 lazy member list**: `sr-gateway` `member_list.rs` (pure build, diff, range parsing, with a randomised convergence test), `lists.rs` (registry per guild and channel, 75 ms coalesced refresh, access re-checks), op 102 in `conn.rs`, `GuildDirectory::member_list` (Postgres implementation in `gateway_pg.rs`: viewers by `channel_permissions`, hoisted role, `MemberObject` payloads, `everyone` flag, 20,000 member cap).
- Docs: docs/05 (guild, channel and gateway sections, 900011), docs/04 (tables), inventory notes, route matrix rows, OpenAPI and TypeScript client regenerated (the committed OpenAPI file was behind the code, so the diff is larger than this branch's routes).

## Still open (frontend items and follow-ups)
- All [FE] items of the file stay open (not in scope). Backend shapes they need are listed above and in docs/05.
- Crossposted copies do not include attachments, stickers or polls (they would need copying files across servers).
- The gateway does not know about the age gate: an account that has not confirmed 18+ still receives `MESSAGE_CREATE` dispatches of an NSFW channel it can view; only the REST endpoints refuse. See the owner questions.
- Servers above 20,000 members get no lazy list (the request is ignored); the paged members endpoint remains.
- Onboarding keeps no "completed" flag; default channels are a client-side filter.
- A temporary member who never reconnects after a server restart keeps the membership until their next disconnect (no startup sweep).
- `PATCH /guilds/{id}/members/{id}` does not take Discord's `mute`/`deaf`/`channel_id`; use the voice-states route.

## Tests run
- `cargo fmt --all -- --check`, `cargo clippy -p sr-core -p sr-storage -p sr-chat -p sr-db -p sr-gateway -p sr-api -p sr-server --all-targets -- -D warnings`, `cargo check --workspace --all-targets`: clean.
- `cargo test -p sr-core -p sr-chat -p sr-storage -p sr-gateway -p sr-api -p sr-db -p sr-server -- --include-ignored --test-threads=1` against the scratch DB: all pass (new: `sr-api/tests/lounge.rs` 7 flows, `routes/settings/tests.rs` 3, `tests/guilds.rs` 3, `routes/messages/tests.rs` 1, `sr-db/tests/lounge_repo.rs` 7, `sr-gateway` 5 protocol tests plus the member list unit and convergence tests, `sr-server` `gateway_pg` member list, plus rule tests in `sr-chat` and `sr-storage`).
- `pnpm --filter @sideroom/api-client typecheck` and `test`: pass after regeneration. The web app was not touched.

## Decisions
- Server tag and vanity URL are free (no boost level); the tag badge is a fixed list of 24 slugs (`sr_chat::server_tag::BADGES`) the client maps to glyphs.
- Pending (unscreened) members get the same read-only permissions as timed-out members; the owner and administrators are never held back.
- The temporary-membership grace is 60 s (Discord removes at once); it spares a reload or a network blink.
- Directory categories are a built-in list of 12 slugs; approved listings of servers that turn Community off are hidden, not deleted.
- Onboarding roles are filtered by `ONBOARDING_FORBIDDEN` (administrator, manage guild/roles/channels/messages/webhooks/threads/nicknames/events/expressions/extensions, kick, ban, moderate, mention everyone) and never managed roles.
- `AUTO_PUBLISH` is channel flag bit 24, outside Discord's range.

## Open owner questions
1. Should the gateway stop dispatching NSFW channel messages to accounts that have not confirmed 18+ (needs an age lookup in the gateway directory), or is the REST gate enough?
2. Is a 60 s offline grace right for temporary members, or should it be immediate like Discord?
3. Are the 12 directory categories right, and should staff manage them (a table) instead of the built-in list?
