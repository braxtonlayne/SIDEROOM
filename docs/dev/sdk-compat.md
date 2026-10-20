# SDK compatibility test list (DEV-SDK-003)

This is the list of behaviours a Discord library must see for a bot to port to SIDE ROOM by changing the base URLs and the token (doc 05 section 7). It has three tiers. Tier 1 runs in CI today; tiers 2 and 3 are the plan for the real libraries.

What is and is not Discord-compatible is spelled out in [specs/developer-platform.md](../specs/developer-platform.md) section 2. Where this list says "differs", the porting guide must say so too.

## Tier 1: in-repo smoke harness (runs now)

`crates/sr-server/src/bot_pg_tests.rs`, test `a_bot_connects_listens_with_the_right_intents_and_answers_commands`. Needs Postgres:

```sh
SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-server --lib bot_pg -- --ignored --test-threads=1
```

It builds the real services (Postgres stores, the real REST router, the real gateway state machine, the real `PgAuthenticator`/`PgGuildDirectory`) and a client that behaves like a library:

| # | Behaviour | Expected |
|---|---|---|
| 1 | `GET /users/@me` with `Authorization: Bot <token>` | the bot user, `bot: true` |
| 2 | `GET /gateway/bot` | `url`, `shards: 1`, `session_start_limit` |
| 3 | IDENTIFY with a wrong, malformed or revoked token | close `4004` |
| 4 | IDENTIFY with `shard: [0, 2]` | close `4010` (differs from Discord: no sharding) |
| 5 | IDENTIFY with `MESSAGE_CONTENT` before the application enabled it | close `4014` |
| 6 | IDENTIFY without privileged intents | `READY` with `user.bot`, `application.{id,flags}`, unavailable `guilds`; no person-only fields (`relationships`, `private_channels`, ...) |
| 7 | `GUILD_CREATE` after READY | the guild with its roles (the bot's managed role) and the bot among `members` with `bot: true` |
| 8 | Someone posts a message; the session without `MESSAGE_CONTENT` | `MESSAGE_CREATE` with `content: ""`, empty `embeds`/`attachments`/`components` |
| 9 | Same message, session with `MESSAGE_CONTENT` | full `content` |
| 10 | Message that mentions the bot, session without the intent | full `content` |
| 11 | `POST /channels/{id}/messages` as the bot | 200, author is the bot with `bot: true`; its own `MESSAGE_CREATE` is never stripped |
| 12 | A bot deleting someone else's message without `MANAGE_MESSAGES` | 403 `50013` |
| 13 | A bot calling account routes (`GET /applications`) | 403 `20001` |
| 14 | Register a global slash command (`POST /applications/{id}/commands`) | 201 with Discord's command object |
| 15 | A member invokes it (`POST /interactions`) | the bot's session gets `INTERACTION_CREATE` with `data.name`, `member`, `token`, `app_permissions` |
| 16 | `POST /interactions/{id}/{token}/callback` type 4 | 204; the message appears in the channel authored by the bot |
| 17 | Reset the bot token | the old token fails on REST (401) and on IDENTIFY (4004); the new one connects |

The route level behaviour (every denial, OAuth2, commands, interaction timing, ephemeral messages, follow-ups) is covered by `crates/sr-api/src/routes/developer/{tests,flow_tests}.rs` and `crates/sr-apps/src/tests.rs` without a database.

## Tier 2: real libraries against a test server (running weekly: discord.js 14.27.0, serenity 0.12.5)

`.github/workflows/sdk-compat.yml` (Mondays 06:00 UTC, or dispatch by hand) runs `node e2e/sdk-compat/run.mjs --lib discord.js|serenity` against a freshly bootstrapped server. The driver creates a server, an application and bot, and installs the bot over REST, then walks seven checks: `ready`, `guild-create`, `command-registered`, `message-content-gated`, `forbidden-delete` (403, code 50013), `slash-command-round-trip` and `message-content-delivered` (a second run with the intent enabled). Results go to `e2e/sdk-compat/report/<library>.xml` (JUnit, uploaded as an artifact) and a failure opens or updates the issue "SDK compat: <library> <version>". Locally: start a server with `SR_SERVER__PUBLIC_URL=http://127.0.0.1:8080`, then `node e2e/sdk-compat/run.mjs --lib discord.js`; serenity lives in its own Cargo project outside the workspace, `e2e/sdk-compat/serenity-bot/` (M-20, D-310: its own `Cargo.lock` and `deny.toml` carry serenity's rustls 0.22 tree and its advisory ignores, so the root workspace never sees them); the driver runs it with `cargo run --manifest-path e2e/sdk-compat/serenity-bot/Cargo.toml`, or set `SDK_COMPAT_SERENITY_BIN` to a built `sideroom-serenity-bot`, and `cargo deny --manifest-path e2e/sdk-compat/serenity-bot/Cargo.toml check` checks it. The versions are pinned exactly (`e2e/sdk-compat/package.json`, `e2e/sdk-compat/serenity-bot/Cargo.toml`) and bumped by hand to the newest release at least two weeks old. discord.py and twilight are not run (later, D-306); the table below is the full plan, the checks above are the subset that runs.

The first local runs (integration/phase17, 2026-10-20) passed 7 of 7 for both libraries only after these fixes, all of them places where SIDE ROOM's payloads differed from Discord's and serenity's typed models refused the frame. Each fix has its own unit test. READY now carries `resume_gateway_url` (it was null). GUILD_CREATE now carries `emojis`, `stickers`, `premium_tier`, `premium_subscription_count`, `premium_progress_bar_enabled`, `nsfw_level`, `guild_scheduled_events` (empty; events come over REST), `member_count`, `large`, `unavailable` and `joined_at`. IDENTIFY and op 3 accept serenity's `since` (serde's `SystemTime` object) instead of closing with 4002. Command objects answer `integration_types: [0]` when none was registered. Interactions carry `attachment_size_limit`. The serenity bot logs serenity's own `tracing` output to stderr (`RUST_LOG`, default `warn,serenity=info`), which is where a refused frame shows up.

Run one sample bot per library against a seeded server (`just serve` plus `sr-cli seed`), with the base URL overridden (`/api/v10` is an alias of `/api/v1`) and `GET /gateway/bot` giving the gateway URL.

| Library | Sample bot | Checks |
|---|---|---|
| discord.js | login, `ready`, `messageCreate` with and without `MessageContent`, `channel.send`, `REST.put(Routes.applicationCommands(id))`, `interactionCreate` + `reply`, `deferReply` + `editReply`, ephemeral reply | the same list as tier 1 through the library's own types |
| discord.py | `on_ready`, `on_message`, `await channel.send`, `app_commands` sync, `interaction.response.send_message(ephemeral=True)`, `followup.send` | gateway v10 handshake, command sync, followups |
| serenity | `EventHandler::{ready, message, interaction_create}`, `create_interaction_response` | typed models must deserialize every field we send |
| twilight | `twilight-gateway` shard 0/1, `twilight-http` create message and commands | strict model deserialization; `shard` handling |

Known library-visible differences to expect (each is in the spec, section 2): the library must not ask for more than one shard; select menus and modal text inputs are not stored on messages (only buttons round trip); editing a posted interaction message changes content only; interactions work in servers only; no HTTP interactions endpoint; `guilds.join`, user installs and teams do not exist.

## Tier 3: conformance fixtures (planned)

`crates/sr-compat-tests` (doc 05 section 7): recorded Discord request/response pairs from a test guild replayed against SIDE ROOM, including the permission-filtered `GET /guilds/{id}/channels` case (D-160). Needs a human with a Discord bot and server to record the fixtures (open question 8).
