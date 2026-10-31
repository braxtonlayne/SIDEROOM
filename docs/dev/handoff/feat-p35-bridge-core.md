# feat/p35-bridge-core: BRG-COR-001, the Discord bridge model, sealed token, outbound webhooks

Branch `feat/p35-bridge-core` (base `integration/phase35`). Migration 1720 only. Real Discord was never contacted:
everything ran against an in-memory Discord (the Rust tests) and `e2e/support/fake-discord.mjs` (smoke-tested by hand).

## Owner decisions applied during the wave (read first)

* **No per-member opt-out** (commander, owner): "if the server or channel is bridged, it is bridged". `bridge_optouts`
  is not in migration 1720 (edited in place; 1720 is unmerged), there is no opt-out route and no opt-out check on the
  outbound path. The channel payload carries `sr_bridge: { direction }` instead (absent when not mapped, the module is
  off or the operator killed the bridge) on `GET /guilds/{id}/channels` and `GET /channels/{id}`. **Not covered:**
  gateway `GUILD_CREATE`/`CHANNEL_*` dispatches build their channel JSON in `sr-gateway` and do not carry it yet.
  **bridge-ui must drop** its opt-out API/UI and `crates/sr-db/src/bridge_dsr.rs` must not touch `bridge_optouts`
  (the table does not exist: its `has_tables` probe checks `bridge_message_map`, so the opt-out queries would fail).
* **Wave 35b readiness**: `bridges.mode` (`selective` default, `whole_guild` reserved; `PUT .../bridge/token` with
  `mode: "whole_guild"` is refused with 400 `BRIDGE_MODE_UNAVAILABLE`) and `bridge_channel_maps.kind`
  (`channel|category|thread`, only `channel` used). No sync was built.

## Done

* `crates/sr-bridge` (new, no new dependencies; `sr-sdk` without default features is the Discord HTTP transport):
  model (`Bridge`, `ChannelMap`, `MessageMap`, `Direction`), `BridgeService`, token connect (bot tokens only: `GET
  /users/@me` must say `bot: true`; sealed with `server.secret_key`, aad = guild id), channel maps with one Discord
  webhook per sending map (webhook token sealed, aad = map id), outbound queue (`bridge_outbox`, one row per webhook
  call, ordered per map, `X-RateLimit-*` honoured, 429 reschedules without spending an attempt, exponential backoff,
  dead letters after 8 attempts), Discord markdown rendering (ids of this server become plain names, custom emoji become
  text, `@everyone` defused, embeds dropped, attachments as signed `/cdn` links, never uploaded,
  `allowed_mentions: {parse: []}`), the clean **teardown** (webhook deletes with three attempts and backoff, failures
  logged by webhook id and remembered in `bridge_teardown_residue`, rows and sealed token deleted in one transaction,
  one `audit_log_entries` row (action 82) with the map count, a system message "The Discord bridge was switched off" per
  channel), the **platform kill** (`teardown_all`: every server, logged per server, each module written off so it stays
  off when the feature returns), member export (ids only) and erasure, and the numbers for Admin > System.
* Event wiring: `crates/sr-server/src/bridge_wiring.rs` feeds the lag-tap bus to `BridgeService::handle_event`
  (mirror create/edit/delete, `ChannelDeleted`, and the **backstop**: any change of `bridge.enabled` or
  `platform.bridge` by any path tears the bridge down) and drains the queue every 400 ms. Discord API root:
  `SR_BRIDGE_DISCORD_API` (default `https://discord.com/api/v10`).
* Switches: `platform.bridge` (platform, default on, client-visible) and `bridge.enabled` (guild, default off, section
  "Integrations, more", **owner_only**) in `crates/sr-features/src/modules/bridge.rs`. Two small trait additions in
  `sr-features`: `Feature::owner_only()` and `Feature::gated_by()` (the module dies and is hidden with its parent
  platform feature), plus `FeatureService::guilds_with_flag`.
* The one feature-toggle handler (`routes/settings/features.rs`) reads `owner_only`: non-owner (including
  `MANAGE_GUILD` and `ADMINISTRATOR`) 403, a bot is refused, and the server's MFA step (`check_level(mfa_level)`) applies.
  Switching off calls `teardown` before answering; the answer carries `teardown: { maps_removed, failed_webhooks: [id] }`
  and every module in `GET .../sr/features` has `owner_only`.
* Routes (`routes/guilds/bridge.rs`, session-only, bots refused by `auth::policy`): class **`guild_owner`**
  `PUT|DELETE /guilds/{id}/bridge/token`, `GET .../bridge/leftovers`; class **`bridge_admin`** (404 while off, 403
  `SR_FEATURE_DISABLED` when the operator killed it) `GET .../bridge/status`, `.../discord-channels`, `.../maps`,
  `POST .../maps`, `PATCH|DELETE .../maps/{map_id}`, `POST .../maps/{map_id}/test`.
* Admin > System tile (`SystemView.bridge`, `System.tsx`, i18n `p35-bridge-core-1.ts`, Vitest).
* `e2e/support/fake-discord.mjs` on 8090 with the REST routes and the control routes bridge-ui assumed
  (`__fake/state`, `__fake/inject`, `__fake/reset`) plus `__fake/control` (rate limit, failures, undeletable webhooks).
  `inject` only records (the MESSAGE_CREATE gateway dispatch is BRG-GW-001 and its fake on 8091).
* `secret_columns.rs` (`crates/sr-db/tests/secret_columns.rs`): `the_bridge_tokens_are_sealed`.

## Shapes that differ from what bridge-ui assumed (`apps/web/src/settings/server/bridge/api.ts`), to reconcile

| bridge-ui assumed | built |
|---|---|
| `GET /guilds/{id}/bridge` status with `state`, `token_set`, `optout_count` | `GET /guilds/{id}/bridge/status`: `{platform_on, module_on, connected, bridge{discord_guild_id, discord_guild_name, bot_user_id, bot_name, mode, status, intents_granted, created_by, created_at, invite_url}, maps[], queued, dead_letters, leftovers[], is_owner}`; maps have `id`, `has_webhook`, `queued`, `dead`, no opt-out count |
| maps addressed by SIDE ROOM `channel_id` | by map `id`: `PATCH|DELETE /bridge/maps/{map_id}`, `POST /bridge/maps/{map_id}/test` (no body) |
| `POST /bridge/test {channel_id}` | see above |
| `PUT /token {token}` | also accepts `discord_guild_id` (needed when the bot is in several servers) and `mode` |
| `DELETE /token` | tears the bridge down (webhooks deleted, maps and token gone), module stays on; returns `{maps_removed, webhooks_deleted, leftovers[]}` |
| `GET /users/@me/bridge/channels`, `PUT|DELETE /channels/{id}/bridge/opt-out` | dropped (owner decision); use `sr_bridge` on the channel |
| `teardown` on the PATCH answer with `failed_webhooks: string[]` | as specified |
| `owner_only` on the module | `GuildFeatureObject.owner_only` |
| `message_content_intent`, `last_error` | `intents_granted` (bit 15 = MESSAGE_CONTENT, written by the gateway half through `BridgeService::set_status`); no `last_error` yet |

## For bridge-gateway

`BridgeService::all_bridges()`, `open_bot_token(guild)`, `set_status(guild, status, intents)`, `add_teardown_hook(hook)`
(stop the shard), `model::map_for_channel`, `message_map_for`; inbound messages must carry `sr_bridge` in the
`MESSAGE_CREATE` event data (that is the loop guard) and insert `bridge_message_map` rows with `direction = 'in'`.

## Not done / decisions

* The MFA step follows the precedent (the server's `mfa_level`, the same `check_level` as the other owner-only
  actions): an owner of a server with 2FA level 0 is not asked. Owner question below.
* Signed links that leave the instance (avatar, attachments) live **24 h** (`SIGNED_LINK_TTL_SECS`); attachment links
  on Discord stop working after a day. Owner question below.
* Webhook usernames containing `discord`/`clyde` get a zero-width space (best effort; Discord's check may differ).
* No replies, reactions, stickers or embeds are mirrored outbound (text, attachment links only).
* The Playwright case is bridge-ui's. The fake Discord was exercised with curl only.

## Tests run

* `cargo test -p sr-bridge -- --include-ignored --test-threads=1` on scratch DB `sr_p35_bridgecore`: 13 unit + 16
  Postgres tests (model defaults, bot-only token and sealing, maps and the E2EE/DM refusal, outbound with identity and
  markdown, edit/delete, loop guard, rate limit headers and 429, backoff and dead letters, teardown, failed webhook
  delete x3 and residue, platform kill, event backstop, export/erasure, erasure delete row from payload, no token in
  reports).
* `cargo test -p sr-features` (136), `-p sr-db --test secret_columns`, `-p sr-api --test route_matrix` (6, with the
  `guild_owner` and `bridge_admin` AUTHZ values and bots refused), `--test openapi_names`.
* clippy `-D warnings` on sr-bridge and sr-features (see the end of this file for the final gates).

## Open owner questions

1. Should the owner's token/module actions demand a fresh second factor even when the server's 2FA level is 0?
2. Lifetime of signed attachment links on Discord (24 h now).
3. Wave 35b: whole-guild mode will need its own consent text (it mirrors roles and permissions).

## CHANGELOG lines

* Added the Discord bridge for servers: the server owner switches it on under Modules, connects their own Discord bot,
  maps channels, and messages flow to Discord with names and avatars. Switching it off removes the webhooks from Discord.
* Operators get a switch for the bridge and a tile with its queue and failures on Admin > System.
