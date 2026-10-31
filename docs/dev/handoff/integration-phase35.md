# Integration, phase 35: the Discord bridge (BRG-COR-001, BRG-GW-001, BRG-UI-001)

Branch `integration/phase35` (base `main` b4d71bca). Not pushed; the commander opens the PR. Real Discord was never
contacted: everything ran against the in-memory Discord of the Rust tests and `e2e/support/fake-discord.mjs` (REST and,
new here, the gateway). Owner decisions and commander defaults of this wave are rows P35-BRG-1 to P35-BRG-8 of
`docs/01-decisions-log.md` (section "Phase 35").

## What merged

| Order | Branch | Merge | Conflicts and how they were resolved |
|---|---|---|---|
| 1 | `feat/p35-bridge-core` (BRG-COR-001) | `--no-ff` | none |
| 2 | `feat/p35-bridge-gateway` (BRG-GW-001) | `--no-ff`, 0dab0f75 | It had cherry-picked three early core commits; every non-`inbound/` file of `crates/sr-bridge` and `1720_bridge.sql` took core's final version (the gateway branch changed only `inbound/*` after the cherry-picks), `lib.rs` kept `pub mod inbound`; `route_matrix.tsv` union |
| 3 | `feat/p35-bridge-ui` (BRG-UI-001) | `--no-ff` | none |

## Gaps closed at integration

- **Inbound runs on a real server.** `crates/sr-server/src/bridge_inbound.rs`: the production `ChatSink` (messages
  written straight through the repository with **no mentions recorded**, `message_bridge_authors` filled before the
  `MESSAGE_CREATE` is built so it carries `sr_bridge`, edits, deletes, reactions as the integration author, typing as
  `TYPING_START` with the Discord id) and `spawn_inbound` in `app.rs` (the shard manager, its supervisor, the teardown
  hook, live `abuse.bridge_messages_per_minute` and `bridge.max_bridges`). A stopped shard now closes its gateway
  connection (it used to stay connected until the next dispatch; `ShardGuard`, test added).
- **Integration author (P35-BRG-6).** Migration **1721** `bridge_integration_authors`: a bot account per server, made on
  the first connect and kept across teardowns; inbound messages are stored under it, never under the owner.
- **Mentions both ways (P35-BRG-2, P35-BRG-8).** Outbound: `<@id>` of a bridged member stays a Discord mention and is
  the only id `allowed_mentions.users` lets ping. Send path: `eligible_mentions` looks `<@id>` up among the server's
  bridged members first (`MessageRepository::bridged_member_ids`), so a colliding id never pings an account. Web:
  the mention resolver and the typing names look at the server's bridged members first.
- **`sr_bridge` on gateway channels.** `GUILD_CREATE` channels carry it (`gateway_pg.rs`), `CHANNEL_CREATE/UPDATE` get it
  from `GuildDirectory::channel_bridge_state`, and the bridge sends a `CHANNEL_UPDATE` when a map is created, changed or
  removed and on teardown (`BridgeNotices::channel_changed`), so banners come and go live.
- **Web reconciled with the real routes.** `apps/web/src/settings/server/bridge/api.ts` reads `GET .../bridge/status`
  and turns it into the screen's view model; maps by map id; the test has no body; revoking the token reloads the
  status; the screen looks again every 3 s while connecting; two strings in `p35-integ-1.ts`. MSW handlers in the
  Vitest use the wire shapes.
- **Opt-out remnants:** none were left after the merge (no table, route, UI or `bridge_dsr` query).
- **Settings:** `abuse.bridge_messages_per_minute` and `bridge.max_bridges` exist once (`sr-admin/src/settings.rs`).
- **Fake Discord gateway.** `e2e/support/fake-discord.mjs` gained a WebSocket gateway written with node built-ins
  (HELLO, IDENTIFY with the fake token or 4004, READY, GUILD_CREATE, heartbeats, op 8 chunk, RESUME answered with an
  invalid session, `deny_intents` to provoke 4014); `/__fake/inject` now emits `MESSAGE_CREATE` (or `TYPING_START`).
- **Config.** `[bridge] discord_api` / `discord_gateway` (`SR_BRIDGE__DISCORD_API`, `SR_BRIDGE__DISCORD_GATEWAY`; the
  older `SR_BRIDGE_DISCORD_API`/`_GATEWAY` env are still read). `SR_BRIDGE_SUPERVISE_SECS` (default 30) is how often
  shards are matched to the database (2 in the e2e run).
- **Bugs the merged tree showed:** two Rust types claimed the OpenAPI name `BridgeObject` (the message's is now
  `MessageBridgeObject`; client regenerated); five `sr-api` feature tests answered 404 because `bridge.enabled` is gated
  by `platform.bridge`, which their registry does not hold (an unregistered parent now gates nothing).
- **Authz matrix.** The three `{map_id}` writes were 404 (no map seeded) and `PUT .../bridge/token` was a 400; the
  world now seeds a bridge and a receive-only map per tenant (token sealed with the world's key) and talks to a closed
  local port as Discord, so every bridge write is admitted by its rightful actor and nothing reaches Discord. **No
  ceiling, `SHAPE_CEILING` or `WRITE_OBSERVED` entry changed.** The 12 failures and "17 weak refusals" the gateway
  agent saw did not reproduce on the merged tree (all six groups proven, 16 weak refusals in `rest` = the ceiling);
  the base was therefore not run.
- **Route matrix (reviewed).** `GET /guilds/{id}/bridge/members` regenerated as bot-denied (every `/bridge` route is
  session-only in `auth::policy`); no `?` cells.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `clippy --all-targets -D warnings`: sr-bridge, sr-sdk, sr-features, sr-api, sr-chat, sr-db, sr-admin, sr-gateway, sr-server, sr-core | pass |
| `cargo test` (Postgres suites `--include-ignored --test-threads=1`, scratch DB `sr_p35_integ`): sr-bridge 57, sr-sdk (all binaries), sr-features 136, sr-chat 394, sr-admin 68, sr-api 895 (then `openapi_names` 2 after the rename), sr-db 343, sr-core 66 | pass (after the fixes above; first runs: sr-bridge 1, sr-api 5 + 1 failures, all caused by the merge and fixed) |
| `sr-server`: lib 238, `immutable_rules` 13, `web_gate` 19, `authz_matrix` 7 incl. `every_route_enforces_its_authz_row` (all six groups) | pass |
| `route_matrix` regenerated (`SR_UPDATE_ROUTE_MATRIX=1`), OpenAPI + api-client + MSW mocks regenerated, `mocks:check` | pass, reviewed |
| `node scripts/lib/gateway-docs.mjs` | no change |
| `pnpm -r --if-present typecheck` (incl. e2e), `lint`, `prettier --check .` | pass |
| Vitest `apps/web` `--maxWorkers=2` | 401 files, 3671 tests, pass |
| `node scripts/build-web.mjs`, `check-entrance-isolation.mjs`, `check-bundle-budget.mjs` | pass |
| Playwright Chromium, dev mode (sr-server 8080 with `memory:` storage and the fake Discord on 8090, Vite 5173): `bridge.spec.ts` | pass (manager sees the greyed card and no entry; owner switches on, pastes the token, maps, posts both ways incl. inbound over the fake gateway with the DISCORD tag, member's post mirrored, switch-off deletes the webhook, closes the gateway, removes banner and entry, posts the system message, a later post goes nowhere). First runs failed on stale spec locators and wording; fixed in the spec |
| Playwright Chromium smoke: `exit-gate.spec.ts`, `fixtures-scenario.spec.ts`, `lounge-shell.spec.ts` | 7 pass, 1 skipped (the bootstrap link test skips on a reused Owner) |

## Not run, and why

- Playwright on Firefox and WebKit (Firefox does not launch on this machine; CI runs the three engines).
- A real Discord: by rule. The first real connection is the owner's (below).
- The base (`b4d71bca`) authz run: not needed, the merged tree passes without touching a ceiling.
- Leftover templates of the gateway agent (`sr_tpl_192_1731_*`, `sr_tpl_193_1732_*`) were left in Postgres (only
  mine, `sr_p35_bridgecore` and its four `sr_tpl_190_1720_*` were dropped, as asked).

## Known limits

- The channel banner names no Discord server (`sr_bridge` carries only the direction).
- With a server secret key that changed since a bridge was connected (for example a dev run without
  `SR_SERVER__SECRET_KEY`), the supervisor warns "sealed value unreadable" for that bridge on every pass until the owner
  revokes and pastes the token again.
- No replies, reactions, stickers or embeds are mirrored outbound (text and attachment links only).

## Owner items

1. **To try it:** create a Discord application with a bot (developer portal), enable the privileged intents
   **MESSAGE CONTENT**, **SERVER MEMBERS** and **PRESENCE**, invite the bot to your Discord server with the invite link
   the bridge screen shows, then on SIDE ROOM: Server Settings > Modules > Discord bridge (as the server owner) > paste
   the bot token > map a channel. Without SERVER MEMBERS / PRESENCE the bridge still works; the "On Discord" list stays
   empty or without presence.
2. The commander defaults P35-BRG-4 to P35-BRG-8 are the owner's to change.
3. Wave 35b (whole-server mode) needs its own consent text; SIDE ROOM is the master copy for permissions (P35-BRG-3).

## PR title

`Phase 35: the Discord bridge (BRG-COR-001, BRG-GW-001, BRG-UI-001)`
