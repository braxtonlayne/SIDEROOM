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

All with `CARGO_TARGET_DIR=.../p13-devplat-target`, `-j 2`, one command at a time, own scratch database `sr_p13hc` (TestDb clones).

- `cargo clippy -D warnings --all-targets` on sr-sdk, sr-core, sr-apps, sr-chat, sr-admin, sr-db, sr-api, sr-cli, sr-server: clean; `cargo fmt --all`.
- `cargo test -- --include-ignored --test-threads=1` on sr-api (564 lib, 18+20+24 integration, route_matrix 5), sr-server (95), sr-admin (42), sr-apps (61), sr-chat (303), sr-core (40), sr-sdk (37): all pass.
- sr-db Postgres: `apps_repo` 19, `rich_store` 4 (new test for the two RichStore lookups), `message_store`, `migration_versions`, `admin_ext_repo` 6, `data_export` 3: all pass.
- sr-cli with Postgres, run twice in a row on the same database: 10 + 4 passed both times (fix (b)).
- sr-voice `ice_tcp_reaches_the_shard_that_owns_the_participant`: 20 of 20 isolated runs and 6 of 6 full `e2e` runs with 4 test threads pass; no flake reproduced, no change made (fix (c)).
- Web: `pnpm -r typecheck`, `pnpm -r lint`, prettier check clean; vitest apps/web `--maxWorkers=2`: 2012 tests passed, the one failing file (`channelPerms.test.ts`, needs the WASM build) passes after `pnpm --filter @sideroom/wasm build`; `@sideroom/bot` 18 tests; `i18n-verify-diff` MISSING 0; doc links ok.
- OpenAPI, `schema.d.ts`, MSW handlers regenerated (808 operations, `mocks:check` ok); route matrix regenerated and reviewed (four emoji rows now `allow` for bots, assets list is `user`), 5 tests pass.

## Not run

- `vite build` and the bundle budget, `cargo deny check`, the full `sr-db` Postgres suite: free space on C: fell below the 40 GB limit set for this task (23 GB at the end, shared by all agents), so I stopped building. The web changes are small (no new route chunks); the `sr-sdk` dependency added is `tracing` (already in the lockfile).
- Playwright (shared ports, per the brief).
