# feat/p35b-sync-gateway (BRG-SYN-003) handoff

Status: FIRST SLICE ONLY (REST layer published). Still open: dispatch handling for structure, the stateful
Rust fake and the e2e fake extension, migration 1756 + display roles, mention translation. See "Open" below.

## DiscordApi method signatures (crates/sr-bridge/src/discord_structure.rs, `impl DiscordApi`)

All take the bot token first; every write takes `reason: &str` (send `reason_for_op(op_id)` = `sr-bridge:<op_id>`),
goes through the structural `Pacer` (`set_struct_writes_per_sec`) and returns `Result<_, CallError>`.
Snowflakes are i64, permission words u64 (the caller masks with BRIDGE_SHARED; this layer never touches bits).

Reads: `guild_info(token, guild) -> GuildInfo` (is_community, role_icons, max_bitrate) |
`bot_access(token, guild, bot_user) -> BotAccess` (administrator, top_position, is_top_role, has(bits)) |
`guild_roles(token, guild) -> Vec<GuildRole>` | `guild_channels_all(token, guild) -> Vec<GuildChannel>` |
`get_channel(token, channel) -> GuildChannel` | `active_threads(token, guild) -> Vec<GuildChannel>`.

Roles: `create_role(token, guild, &RoleParams, reason) -> GuildRole` | `edit_role(token, guild, role, &RoleParams, reason)` |
`delete_role(token, guild, role, reason)` | `set_role_positions(token, guild, &[(role_id, position)], reason)`.

Channels/categories/overwrites: `create_channel(token, guild, &ChannelParams, reason) -> GuildChannel` (overwrites ride in the body) |
`edit_channel(token, channel, &ChannelParams, reason)` (also threads: archived/locked/applied_tags; forum `available_tags`) |
`set_overwrites(token, channel, &[Overwrite], reason)` (whole list) | `put_overwrite(token, channel, Overwrite, reason)` |
`delete_overwrite(token, channel, target, reason)` | `delete_channel(token, channel, reason)` |
`set_channel_positions(token, guild, &[ChannelPosition], reason)`.

Threads/forums: `create_thread(token, channel, &ChannelParams, reason)` |
`create_forum_post(token, forum, &ChannelParams, first_message, reason)` |
`execute_webhook_in(id, wtoken, &WebhookPost, &ThreadTarget{thread_id,thread_name,applied_tags}) -> PostedMessage{message_id,channel_id}` |
`edit_webhook_message_in(id, wtoken, message, thread_id, content, ping_users)` | `delete_webhook_message_in(id, wtoken, message, thread_id)`.

Other: `invalid_requests_within(Duration) -> usize` (breaker input: 401/403/429), `blocked_for(&channel_key(id) | &guild_key(id))`,
`is_bridge_reason(Option<&str>)` (echo test for GUILD_AUDIT_LOG_ENTRY_CREATE), `intents::for_mode("selective"|"whole_guild", presences)`,
`channel_type::*`, `perm::*`. Types are plain (no sync-core types needed): RoleParams, ChannelParams (Option fields; `Option<Option<_>>` clears),
ForumTag, Overwrite{id,kind(0 role,1 member),allow,deny}, ThreadState.

## Open (not done yet)
inbound/structure.rs dispatch -> "look at object" sink; intents wired into the shard manager by mode; the stateful fakes
(sr-bridge tests/support, e2e/support/fake-discord.mjs on 8094); migration 1756 `bridge_members.discord_role_ids` + display_roles;
role/channel mention translation in render.rs.

## Tests run
`cargo test -j 2 -p sr-bridge --lib discord`: 10 passed (body building, reason encoding, intents, parsing, pacer).
