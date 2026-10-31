# feat/p35b-sync-gateway (BRG-SYN-003) handoff

Status: row built except the items under "Open". Branch `feat/p35b-sync-gateway`, base integration/phase35b.

## DiscordApi method signatures (crates/sr-bridge/src/discord_structure.rs, `impl DiscordApi`)

All take the bot token first; every write takes `reason: &str` (send `reason_for_op(op_id)` = `sr-bridge:<op_id>`),
goes through the structural `Pacer` (`set_struct_writes_per_sec`, default 2/s) and returns `Result<_, CallError>`.
Snowflakes are i64, permission words u64 (callers mask with BRIDGE_SHARED; this layer never touches bits).

Reads: `guild_info(token, guild) -> GuildInfo` (is_community, role_icons, max_bitrate) |
`bot_access(token, guild, bot_user) -> BotAccess` (administrator, top_position, is_top_role, has(bits)) |
`guild_roles` | `guild_channels_all` | `get_channel(token, channel)` | `active_threads(token, guild)`.

Roles: `create_role(token, guild, &RoleParams, reason)` | `edit_role(token, guild, role, &RoleParams, reason)` |
`delete_role(token, guild, role, reason)` | `set_role_positions(token, guild, &[(role_id, position)], reason)`.

Channels/categories/overwrites: `create_channel(token, guild, &ChannelParams, reason)` (overwrites ride in the body) |
`edit_channel(token, channel, &ChannelParams, reason)` (also threads: archived/locked/applied_tags; forum `available_tags`) |
`set_overwrites(token, channel, &[Overwrite], reason)` (whole list) | `put_overwrite` | `delete_overwrite` |
`delete_channel(token, channel, reason)` | `set_channel_positions(token, guild, &[ChannelPosition], reason)`.

Threads/forums: `create_thread` | `create_forum_post` |
`execute_webhook_in(id, wtoken, &WebhookPost, &ThreadTarget{thread_id,thread_name,applied_tags}) -> PostedMessage{message_id,channel_id}` |
`edit_webhook_message_in(.., thread_id, ..)` | `delete_webhook_message_in(.., thread_id)`.

Other: `invalid_requests_within(Duration)` (breaker input: 401/403/429), `blocked_for(&channel_key(id) | &guild_key(id))`,
`is_bridge_reason(Option<&str>)`, `intents::for_mode(..)`, `channel_type::*`, `perm::*`.
Types are plain (no sync-core types): RoleParams, ChannelParams (`Option<Option<_>>` clears), ForumTag, Overwrite, ThreadState.

## Done
- REST layer above. Audit-reason header (percent-encoded, 512 chars), pacer, invalid-request counter.
- **Stateful fake** `crates/sr-bridge/tests/support/fake_guild.rs` (`FakeGuild`, an `HttpTransport`, no Postgres): roles with
  positions/managed/Discord-only bits, all channel types, overwrites, threads, forum tags (ids assigned), webhooks incl. thread
  posts, bot-below-top-role and grant-only-held-bits rules (admin exempt), 250/500 caps, Community-only types, bitrate tier clamp,
  two renames per channel per window (3rd = 429, `reset_rename_window()`), `rate_limit_next(n, global)`, every write recorded
  with its decoded reason, gateway dispatches in `drain_events()`, audit entries with the reason echoed (`audit()`),
  `discord_admin(...)` to edit as a Discord user, `seed_role/seed_channel`. Use via `#[path = "support/mod.rs"] mod support;`
  then `support::fake_guild::{FakeGuild, BASE}` and `DiscordApi::with_transport(BASE, Arc<FakeGuild>)`.
  Tests: `crates/sr-bridge/tests/structure.rs` (8).
- **Dispatch handling** `inbound/structure.rs`: `events_for(name, data) -> Vec<StructureEvent>` (role/channel/thread/THREAD_LIST_SYNC),
  `StructureSink` port (`look_at`, `attribute`), `Inbound::with_structure(sink)`. Audit entries with an `sr-bridge:` reason are
  dropped (`Why::Echo`); other audit entries call `attribute(target, discord_user, action)` (needs the GUILD_MODERATION intent).
  Events of other Discord servers are dropped; with no sink attached structural events are ignored (`Why::NoReconciler`), so wave-35
  selective bridges are unchanged. **sync-core must implement `StructureSink`** (enqueue a Discord-to-SR op per event; decide
  mode/map filtering there) and attach it where `Inbound::new` is built in sr-server.
- **Intents per mode**: `inbound::intent_ladder_for_mode(mode, presence)`; whole_guild adds GUILD_MODERATION on every rung;
  the manager uses `bridge.mode` at shard start. **A mode switch must restart that bridge's shard** (sync-core: stop and start the
  shard after `bridges.mode` changes) for the new intents to apply.
- **Migration 1756** `bridge_members.discord_role_ids BIGINT[]` (filled by `members::upsert`); `inbound::members::display_roles(ids, map)`
  pure function (display only). 1757 unused. NOTE: sr-db embeds migrations at compile time and cargo does not notice a new file;
  `touch crates/sr-db/src/lib.rs` after adding one or Postgres tests fail with a missing column.
- **Mention translation**: outbound `Names.role_ids` / `Names.channel_ids` (SR id to Discord id; mapped `<@&id>` / `<#id>` go out as
  real mentions, still never ping since `allowed_mentions.parse = []`); inbound `convert::clean_content_mapped(raw, mentions, &MentionMaps)`.
  Both default to empty (selective unchanged). **Not yet populated**: loading `bridge_role_maps` / channel maps into `Names`
  (outbound.rs payload build) and `MentionMaps` (inbound handler) belongs with sync-core's maps.
- **Rename bucket**: the 429 on a third rename blocks `channel_key(id)` via `blocked_for`; "only the latest name is sent" is the op
  queue's debounce (sync-core). Both fakes model the limit.
- **e2e fake**: `e2e/support/fake-discord-structure.mjs` (mounted by `fake-discord.mjs`, same rules as the Rust fake, node built-ins only),
  run with `SR_FAKE_DISCORD_PORT=8094`; control routes `/__fake/structure{,/admin,/control,/seed}`; helpers in `e2e/support/bridgeFake.ts`
  (`fakeStructure`, `fakeAdminEdit`, `fakeStructureControl`, `fakeSeed`); smoke test `node --test e2e/support/fake-discord-structure.test.mjs`.
  Wave-35 routes unchanged (GUILD_CREATE now carries the fake's roles/channels; webhook posts return `channel_id`).

## Open
- Populating `Names`/`MentionMaps` from role and channel maps, and attaching a `StructureSink` in sr-server (sync-core).
- The instance-wide 2,000 invalid-request breaker needs a counter shared across `DiscordApi` clones per bridge; the per-client counter exists.
- No Playwright run (ports 8080/5173 belong to sync-ui); the Node fake was only smoke-tested directly. The wave-35 `bridge.spec.ts` was not re-run against the modified fake.
- Audit-log REST read (`GET /guilds/{id}/audit-logs`) not added; attribution relies on the gateway entry.

## Tests run
`cargo test -j 2 -p sr-bridge -- --include-ignored --test-threads=1` on scratch DB sr_p35b_syncgw: lib 40, db 16, inbound 15, structure 8, all pass.
`cargo fmt --check -p sr-bridge` and `cargo clippy -j 2 -p sr-bridge --all-targets -- -D warnings` clean.
`node --test e2e/support/fake-discord-structure.test.mjs`: 3 pass. Prettier clean on the e2e files.

## Owner questions
None new. Relied on Q7 (active threads only) and Q9 (voice structure only).
