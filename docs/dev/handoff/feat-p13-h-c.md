# Handoff: feat/p13-h-c (H-28 to H-32 and four integration fixes)

Branch `feat/p13-h-c`, base `integration/phase13`. Local only: nothing pushed, no PR.

## Done

- **H-28 SDK address rule (D-234).** `sr-sdk` follows the gateway URL from `GET /gateway/bot` and `resume_gateway_url` from READY
  only on the base host or a host added with `ClientBuilder::allow_gateway_host`; anything else logs at `warn` and uses the
  configured gateway URL (`wss://<base host>/gateway`, or an explicit `gateway_url`). Same rule in `@sideroom/bot`
  (`allowGatewayHosts`). `deny.toml` and the crate manifest cite D-234, doc 03 has the row, `crates/sr-sdk/README.md` says the SDK
  never fetches message-content URLs. Tests: socket-double tests for an off-host resume URL (falls back) and an allow-listed one
  (followed), the same for `/gateway/bot`.
- **H-29 purge webhook author rows (D-235).** `PURGE_SQL` deletes `message_webhook_authors` in the same statement. Test in the
  `sr-server` hold-guard Postgres test (rows of purged messages go, held and not-yet-due rows stay, a released hold's row goes on the
  next purge). Account deletion never removes messages (rows are kept for the erasure record), so there was nothing to change in
  `sr-db` there.
- **H-30 "via webhook" marker (D-235).** `PrivateMessage.webhook {id, name}` on the staff private-content read (admin viewer shows
  "via webhook <name>"), `webhook` on every data-export message row, `webhook_id` in the `options` of a `MESSAGE_DELETE` audit entry
  made by someone other than the creator. Doc 05 section 5.3 row. Tests: `sr-db` Postgres (`admin_ext_repo`, `data_export`),
  `sr-api` audit test, web viewer test. **Not applicable yet:** the member drawer's message history and the moderation case message
  view do not exist in this tree, so there was nothing to mark there; they should read the same `webhook` marker when built.
- **H-31 application emoji usable (D-237).** (a) `/applications/{id}/emojis` routes accept the owner's session or the bot of that
  application; another application's bot gets 403 (`50001`); credential policy and matrix updated. `GET /applications/{id}/assets`
  is now readable by any session (D-237), writes stay owner-only. (b) `decorate` adds `sr_text_emoji` to a message that names an
  application emoji but was not sent by that application's bot; the web message body shows those as `:name:` text. (c) `GET
  /sr/emojis/{id}` and `/url` fall back to `application_emojis` (`application {id, name, icon}`, `guild: null`); the popout says
  "Emoji of <application>" with no join. (d) The Emojis tab note is gone. Tests: `sr-api` developer/settings/messages tests, web
  popout and view-model tests.
- **H-32 SDK gateway ops.** `sr-sdk` `Client::gateway()` -> `GatewayHandle::{request_guild_members, update_presence}` (op 8 with a
  typed `MembersChunk` stream matched by nonce, op 3), builders, shard routing by `(guild_id >> 22) % shards`; `@sideroom/bot`
  `requestGuildMembers` (async generator) and `setPresence`. Inventory 17: DEV-PORT-003 re-scoped, DEV-SDK-003 and SDK gap order
  noted.
- **(a) Share to Lounge picker.** A server channel is offered only when the person holds `SEND_MESSAGES` and `EMBED_LINKS` there
  (shared permission engine when loaded, guild bits otherwise; nothing is hidden until the member is loaded; DMs unchanged).
- **(b) sr-cli test.** `staff_roles_are_created_once_and_repaired_on_rerun` now uses a `TestDb` template clone.
- **(c) sr-voice `ice_tcp_reaches_the_shard_that_owns_the_participant`:** see "Tests run".
- **(d) Epoch.** `apps/web/src/lounge/epoch.generated.ts` is generated from the Rust constant; the sr-core test
  `web_epoch_file_is_generated_from_rust` writes it (`SR_UPDATE_GENERATED=1`) and fails on drift. No hand-written TypeScript copy is
  left (`liveMap.ts` and the three callers import the generated file).

## Still open / owner questions

- **Server side of op 8.** The gateway routes `REQUEST_GUILD_MEMBERS` to registered handlers and none is registered, so a real server
  does not answer with `GUILD_MEMBERS_CHUNK` yet. The SDK side is built and tested against the socket double; a server handler is a
  follow-up (inventory text says both ops are "already served"; only op 3 is).
- **USE_LIBRARY_EMBEDS.** D-226 names this bit for the Share picker, but the server never checks it and neither `@everyone` nor DM
  permissions grant it, so filtering on it would empty the picker. The picker uses the bit the message pipeline checks for embeds
  (`EMBED_LINKS`). Owner: should the server enforce `USE_LIBRARY_EMBEDS` for `<library:id>` / `<cinema:id>` cards (and grant it by default)?
- **Audit actor for bot-managed application emoji.** A bot acts for the application's owner, so `app.emoji_*` audit rows name the
  owner, not the bot user.

## Tests run

See the final report of the agent for the exact list (commands and counts).
