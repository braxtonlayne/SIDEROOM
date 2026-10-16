# Handoff: feat/p14-fu-gw (H-44, H-50, H-51, H-52, H-10)

Branch `feat/p14-fu-gw` off `integration/phase14`. Decisions D-241 and D-242 as written.

## Done

- **H-44** `voice.shards` defaults to `1` (`0` keeps the auto rule). `crates/sr-core/src/config.rs`, test in `voice_is_off_by_default_and_validated_when_on` (also checks `shards = 0` still parses). `docs/dev/server.md`, `docs/specs/lounge-voice-video.md` 4.12 and inventory 08 say "1 until the AX42 run is recorded, then 0".
- **H-50** `USE_LIBRARY_EMBEDS` (bit 58) enforced at send and default-granted.
  - Migration `0876_use_library_embeds_default.sql` (forward-only, @everyone roles, like 0860); `perms.rs` adds the bit to the new-server default and `dm_permissions()`.
  - `sr-markdown` `Mentions` gains `cards` (count of `<library:id>` / `<cinema:id>` outside code), so the same parser pass that checks limits finds them; `message_rules.rs` requires the bit on create (50013) and on edit when the edit adds a card (fixing a typo next to an existing card is not blocked).
  - Web: `lounge/liveMap.ts` `PERM.USE_LIBRARY_EMBEDS`; `lib/ShareToLounge.tsx` filters on `SEND_MESSAGES | USE_LIBRARY_EMBEDS`.
  - Tests: `message_rules/tests.rs` (refused without, accepted with, DM accepted, code span not a card, edit cases), `sr-db/tests/p14_followups.rs` (0876: grants once, other roles untouched, idempotent, new servers have it), `lib/shareToLounge.test.tsx`. Docs: doc 07 row 58, doc 05 note.
- **H-51** audit actor for application emoji. `sr-apps` `add_app_emoji_as`, `rename_app_emoji_as`, `delete_app_emoji_as` take an `actor`; the old methods delegate with actor = owner. `app_media.rs` passes `p.user_id` (the bot user for a bot token, the owner for a session); `emoji_owner` unchanged. Test `emoji_audit_rows_name_the_bot_or_the_owner`.
- **H-52** op 8 server side (D-242).
  - `crates/sr-gateway/src/members_request.rs`, called from `conn.rs`. It is not an `OpHandler` because it has to close the connection (4002 / 4014), which a handler cannot do.
  - Payload: `guild_id`, `query` xor `user_ids` (at most 100), `limit`, `presences`, `nonce` (at most 32 bytes); malformed is 4002. Non-snowflake ids go to `not_found`.
  - Gating: the empty query needs a bot session with `GUILD_MEMBERS` (user sessions: 4014), `presences: true` needs `GUILD_PRESENCES` (4014). Membership: the session's guild set (installed guilds for a bot) and shard ownership; a request for another guild is ignored.
  - `GuildDirectory::request_members(guild, MemberRequest)` (new, default `None`). Postgres impl in `sr-server/src/gateway_pg.rs` uses the same `list_members` search the REST route uses (substring over nick, username, display name), `get_member` for ids, `MemberObject` without `sr_join` / `sr_signals`.
  - `GUILD_MEMBERS_CHUNK` of at most 1000, `chunk_index`, `chunk_count`, `not_found` (first chunk), `presences` from the live presence map (online and visible only), echoed `nonce`. An empty result is one empty chunk so a waiting client always hears back. Counts as one op against the existing 120 / 60 s budget.
  - Tests: `sr-gateway/src/members_request_tests.rs` (payload parsing, query, ids with a missing id, whole list with and without the intent, user session refused the whole list, chunking at 1001, presences and their intent, 4002, stranger guild ignored, op before IDENTIFY), `gateway_pg::request_members_searches_by_name_and_ids` (Postgres), and `crates/sr-sdk/tests/op8_real_gateway.rs`: the sr-sdk `Client` drives op 8 against the real `run_connection` through an in-memory pipe (1001 members arrive as 2 chunks, ids with a missing one). `sr-sdk` gets `sr-gateway` (test-support) and `sr-core` as dev-dependencies only.
  - Docs: doc 05 section 6.1 row and new 6.6a, inventory 17 note "op 8 served (H-52)".
- **H-10** `crates/sr-api/tests/openapi_names.rs`: scans every `ToSchema` derive in `crates/*/src` and fails with both type paths when two Rust types share a schema name. `LinkBody` was already renamed.

No route or schema changed, so OpenAPI, the generated client, the MSW mocks and `route_matrix.tsv` needed no regeneration (op 8 is a WebSocket op).

## Still open

- **17 pre-existing schema-name clashes** found by H-10: `ApplicationObject`, `AuditEntryObject`, `BanBody`, `Bookmark`, `BookmarkEntry`, `ChangeEmailBody`, `CreateInviteBody`, `DecisionBody`, `FollowBody`, `InstantSearch`, `MergeBody`, `PartUrl`, `ReauthBody`, `RenameBody`, `UploadRequest`, `UploadSlot`, `UserRef`. They are in `KNOWN_CLASHES` in the test. For several the committed `openapi.json` shows one type's fields only (e.g. `CreateInviteBody` has only `note`), so the generated client is wrong for the other type. Each needs `#[schema(as = ...)]` on one side, regenerated OpenAPI / client / mocks and a look at the web callers; some are internal types that never reach the served document (`sr-media` `PartUrl`, `UploadRequest`, `UploadSlot`), which the check cannot tell apart. Worth a follow-up task of its own; removing a name from the list is enforced once it stops clashing.
- H-44 stays at 1 until the AX42 numbers are recorded (owner/hardware).

## Decisions and judgment calls

- Empty `query` is treated as "the whole list" for gating whatever `limit` is (stricter than `limit: 0` only); a non-empty `query` with `limit` 0 returns at most 100.
- User sessions asking for the whole list get 4014 (D-242 says they may use `query` and `user_ids` only; it does not name the refusal).
- A request for a guild the session is not in is ignored (as op 102 does), not closed.
- Search is the REST substring search, a superset of Discord's prefix match.
- The whole list is built in memory in 1000-row pages before chunking (chunk_count must be known up front); fine for a friend-group instance, a cursor would be needed for very large servers.
- Edits: the bit is re-required only when the edit adds a card, so existing cards (pre-0876 servers where an admin cleared the bit) do not lock the author out of fixing a typo.

## Tests run

`cargo test -j 2`: sr-core, sr-markdown, sr-chat, sr-gateway (166), sr-sdk (37 + the new integration test), sr-api (all targets, 573 lib tests), sr-db `p14_followups` and sr-server `gateway_pg` against a scratch Postgres database (created and dropped). `cargo fmt --check`; `cargo clippy -j 2 --all-targets -D warnings` on sr-core, sr-markdown, sr-chat, sr-gateway, sr-sdk, sr-apps, sr-db, sr-api, sr-server. Web: prettier, eslint on the touched files, `tsc --noEmit`, vitest `shareToLounge.test.tsx`. Not run: full web suite, build / bundle budget (a two-line change in two existing files), Playwright (shared ports).

## Owner questions

None new. The 17 clashes above are an engineering follow-up, not an owner decision.
