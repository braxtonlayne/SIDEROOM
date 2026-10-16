# Handoff: feat/p13-devplat (developer platform: SDKs, directory, portal tabs, webhooks)

Branch `feat/p13-devplat`, based on `integration/phase13`. Migrations `0850` and `0851`. Local commits only. Spec: [developer-platform.md](../../specs/developer-platform.md) (sections 2 and 3.9). Inventory: [17-developer-platform.md](../../inventory/17-developer-platform.md). D-185, D-186, D-204 to D-206 and D-220 are untouched. G-85 is not mine.

## Audit result

Unticked boxes of inventory 17 before this phase, and what I found:

| ID | Found | Result |
|---|---|---|
| DEV-SDK-001 | `crates/sr-sdk` was an empty stub | built, ticked |
| DEV-SDK-002 | no package | built, ticked |
| DEV-BOT-006 | execute route ignored `username`/`avatar_url`/`thread_id`; messages were attributed to the webhook creator | built, ticked |
| DEV-BOT-008 | attachment option refused by the server and the form | built, ticked |
| DEV-BOT-012 | nothing | built, ticked |
| DEV-PORT-003 | Emojis, Webhooks, Rich Presence, Activities tabs missing | Emojis and Rich Presence built; box stays open |
| DEV-PORT-004 | hand-written hub | SDK and porting pages added; box stays open (not generated from OpenAPI) |
| DEV-BOT-007, DEV-APP-007, DEV-SDK-003, DEV-ACT-*, EXT-* | not built | untouched, reasons below |

## Done

- **DEV-SDK-001, Rust `sr-sdk`.** `http.rs` (transport trait, `Rest`, 429 retry with `Retry-After`/`retry_after`, typed message/command/interaction helpers; the bot token never reaches `Debug`), `gateway.rs` (HELLO, IDENTIFY with shard and intents, RESUME on the resume URL, heartbeat with zombie detection, `RECONNECT`, `INVALID_SESSION`, fatal close codes 4004/4010 to 4014, backoff; `WsConnector` over `tokio-tungstenite` with rustls and native roots), `cache.rs`, `command.rs` (builders for slash, user and message commands, options incl. attachment, sub-commands, typed `Args`, autocomplete, reply/defer/follow-up/edit helpers), `client.rs` (`Client::builder`, `Shards::{Auto, Count, One}`, one IDENTIFY every 5 s, commands registered at the first READY, a failing handler sends the person a private error and calls `on_error`). `examples/echo_bot.rs`. 24 tests including a real-socket gateway test. A race was found and fixed by a test: when a shard ends with a fatal close the dispatch channel can close before the join result is read, so `start` now drains the shard tasks for the error.
- **DEV-SDK-002, `@sideroom/bot`** (`packages/bot`): `Client` (typed events `ready`, `messageCreate`, `interactionCreate`, `raw`, `error`, `fatal`), `Rest`, `Shard` (injectable socket, global `WebSocket` by default), `Cache`, discord.js-like `SlashCommandBuilder` and interaction classes, `CommandRouter`. 15 vitest tests, example `examples/echo.ts`.
- **DEV-BOT-006.** Migration `0850` `message_webhook_authors`; `RichStore::{set_webhook_author, webhook_authors}` (default no-ops, Postgres and memory implementations); `rich_model::decorate` renders a webhook message with `author.id` and `webhook_id` = the webhook id, name = override or webhook name, `bot: true`, `author.avatar_url` = image-proxy path (never the raw URL; none without a CDN signer). `username` 1 to 80 characters, `avatar_url` https at most 512 characters, bad values are `50035` before anything is posted. `?thread_id=` posts into a thread of the webhook's channel (`50035` for anything else). Web: the Lounge shows the name and avatar, a webhook author has no profile popout.
- **DEV-BOT-008 attachments.** `Referenced.attachments`; `POST /interactions` resolves each option through the staged upload (`load_upload`, so only the invoker's own slot in this channel; completed with `finalize` when needed; refused when already on a message) into `data.resolved.attachments` with a signed URL. The command form (`CommandForm.tsx`) uploads the file first and sends the upload id.
- **DEV-BOT-012 App Directory.** `GET /app-directory`, `/app-directory/tags`, `/app-directory/{id}` (session only), backed by `AppsRepository::{directory, directory_tags}` (Postgres query escapes `LIKE` wildcards). Web: Server Settings, Integrations, "Bots and Apps" (replaces the "not available" text): search, tag chips, paging, app page with commands and links, "Add App" for people with Manage Server (plain link to `/oauth2/authorize` with the app's scopes and permissions and this server).
- **DEV-PORT-003 Emojis and Rich Presence tabs.** Migration `0851` (`application_emojis`, `application_assets`); owner-session routes under `/applications/{id}/emojis` and `/assets` (data URI upload, sniff, shrink, caps 2000 and 300, unique names, audit rows, image removed on delete); portal tabs with upload, rename, delete and thumbnails.
- Docs: portal "Bot SDKs" and "Porting from Discord" pages, a stale "no HTTP interactions endpoint" sentence fixed; spec sections 2 and 3.9; inventory notes. Route matrix rows (no `?` left), OpenAPI, api-client schema and MSW handlers regenerated. i18n fragments `p13-devplat-1.ts` to `-3.ts`.

## Still open (and why)

- **DEV-BOT-007 bots in voice.** Needs WebRTC and an MLS client in the SDK (doc 11 §3 item 6); the MLS voice client is not built, so a signalling-only API would not let a bot hear or speak.
- **DEV-PORT-003 Webhooks tab.** Discord's tab is the event-subscription URL (webhook events), a feature with no spec or backend here. Channel webhooks stay in Server Settings.
- **DEV-PORT-003 Activities tab, DEV-ACT-001 to 004.** Need the `<app-id>.activities.<host>` proxy (wildcard host, reviewed bundle hash, URL mappings), instances, the embedded-app SDK and the iframe host. No small first slice that stands alone.
- **DEV-PORT-004** stays open: the pages are hand-written, not generated from OpenAPI.
- **DEV-APP-007 user installs** (owner decision, unchanged), **DEV-SDK-003** (needs discord.js, discord.py, serenity and twilight in CI).
- Application emoji cannot be used yet: a bot cannot send `<:name:id>` of one, clients do not render it, and bots (as opposed to the owner's session) cannot manage them. The Emojis tab says so.
- `message_webhook_authors` rows are not removed when old messages are purged (they hold only a name and a URL).
- Uploads staged for an attachment option and never used by a posted message stay as ordinary unattached upload slots (same lifetime rules as an abandoned composer upload).
- The Rust SDK has no presence update, voice, `REQUEST_GUILD_MEMBERS` or HTTP-interactions server; the TS SDK the same.

## Tests run

- `cargo test -j 2` on `sr-sdk` (24), `sr-apps` (61), `sr-chat` (285), `sr-api` (525 lib, route matrix 3, plus the other integration targets), all `--offline`.
- Postgres, own scratch DB, `--test-threads=1`: `sr-db` `apps_repo` (20, new: emoji/assets, directory), `rich_store` (3, new: webhook authors), `secret_columns`, `account_deletion`, `media_columns`, `db`, `data_export`.
- `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on `sr-sdk sr-apps sr-chat sr-db sr-api`; `cargo deny --offline check bans licenses sources`.
- Web: `tsc`, `eslint`, `prettier --check`, vitest for `src/i18n src/developers src/settings/server src/lounge/apps src/lounge/__tests__ src/styles src/lib src/store` (one load-sensitive timeout in `people.test.tsx` passed on rerun), `packages/bot` tsc, eslint, vitest; `vite build` and `scripts/check-bundle-budget.mjs` (developers 213.6 KB of 400, settings 260.2 of 600, lounge 360.8 of 600).
- No Playwright, no server started.

## Decisions and owner questions

1. **`reqwest` in `sr-sdk` (D-203).** The SDK talks to the server its operator names, not to user-supplied addresses, so I added `sr-sdk` to the `wrappers` list in `deny.toml` instead of writing a second HTTP client. Confirm, or ask for a `SafeClient`-style restriction (the transport is a trait, so swapping it is easy).
2. **Webhook author identity.** `author.id` is the webhook id (Discord parity), `messages.author_id` stays the creator. The creator's permissions applied at send time; if the creator is deleted or loses access, existing webhook messages stay as they are.
3. **Webhook avatar override is only shown through the image proxy** (privacy: viewers never contact the avatar host); without a configured signer no avatar is returned.
4. **Directory categories are the applications' tags** (up to 5 each); no new category field. Only `released`, non-suspended apps with `bot_public` are listed; no review of the listing text beyond the release review.
5. **Application media is owner-session only**, not bot-callable (Discord lets bots manage application emoji).
6. Attachment options need `ATTACH_FILES` in the channel (the slot request checks it); Discord does too.
