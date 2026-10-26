# Permission-shape cache (P24-PERF-1)

Status: designed, not built. Fable decided it on the owner's behalf on 2026-10-25 during the wave-24 integration. Build it only if `load-test.yml` at the defaults still misses a target after the two measured fixes: Postgres reached at the container address instead of through docker-proxy, and 16 statements per message instead of 25 (see `load-testing.md`). The decision is in `../01-decisions-log.md` (P24-PERF-1).

## Why it is narrow

Of the 16 statements per message, 6 read the same guild row, role list and overwrite list twice. `POST /channels/{id}/messages` reads them in `ChatService::access`, and `dispatch_audience` reads them again for the same message a few milliseconds later. A 60-server READY would save 180 of its 1,295 statements.

The project already has three permission-adjacent caches that follow the same rule: writes invalidate at once, and a TTL only bounds staleness across nodes.

- `sr-search::access::ViewableChannelCache`
- the `sr-features` cache
- AutoMod's `RuleCache`

This cache reuses that rule rather than adding a different one. Nothing per user is cached, so a revocation of a single user's access never depends on invalidation.

## Deployment model

One node. `sr-events` ships only `InProcessBus`; `PgNotifyBus` is listed under Later in the roadmap (doc 02 §5, doc 13). The design must not make a future multi-node bus harder.

## Design (phase 1)

`sr_db::shape::GuildShapeCache` plus a `CachedChatRepo<R>` decorator around `PgChatRepo`. It is built once in `app.rs` and used by both `ChatService` (REST) and `PgGuildDirectory` (`can_view_channel`, `has_guild_permission`, `dispatch_audience`, `build_guild_create`). Wrapping the repo rather than its callers keeps every reader consistent.

| Key | Value | Source | Bound |
|---|---|---|---|
| `GuildId` | `Arc<Guild>` | `get_guild` | 4,096 |
| `GuildId` | `Arc<Vec<Role>>` | `list_roles` | 4,096 |
| `ChannelId` (the channel, or a thread's parent) | `Arc<Vec<Overwrite>>` | `get_overwrites`, `get_overwrites_of` | 32,768 |

All three use moka `sync::Cache` with a 60 s `time_to_live`, the `ACCESS_TTL` figure. Every entry stores the guild generation it was loaded under. An entry behind the current generation counts as a miss.

**Never cached:**

- channel rows (`last_message_id` is written on every message, and thread archived and locked state lives there)
- `get_member` and `get_members`
- `member_roles`
- timeouts and pending
- bans
- `is_thread_member`
- `is_recipient`
- accounts and flags
- entitlements and features (each has its own cache)
- anything keyed by user

**Invalidation.** All three layers are required.

1. **Write-through.** A shared `Arc<GuildGenerations>` (`bump`, `current`) is handed to every sr-db repo. Every method that writes `guilds`, `roles` or `permission_overwrites` bumps before it returns. The writers found so far:
   - `chat_repo.rs`: 450, 490, 506, 716, 724, 749, 772, 898
   - `guild_admin_repo.rs`: 119, 134 (PATCH, including the owner), 174, 397, 414, 429, 453, 467, 474
   - `apps_repo.rs`: 1467, 1903–1922, 2121
   - `payments_repo.rs`: 1157
   - `settings_repo.rs`: 1242
   - `invite_join.rs`: 199
   - `admin_repo.rs`: 682 (`transfer_owner`, which publishes no event)
2. **Event hook.** `LagBus::publish` calls `generations.apply_event` before forwarding, for `GuildUpdated`, `GuildDeleted`, `Role*`, `Channel*` and `Thread*`. The same hook runs on a `Topic::Firehose` subscription, so events received from another node invalidate too.
3. **TTL of 60 s.** This bounds only writes made outside the process: `sr-cli` guild bootstrap, `psql`, or a second node.

**Load race.** Read `current(guild)` before the SELECT and insert only if it is unchanged afterwards, as `sr-features` does.

**Staleness:**

| Write | Visible |
|---|---|
| On this node, through any repo or event | at the next statement |
| Direct SQL, or another node | within 60 s |
| A per-user revocation (kick, ban, timeout, a role removed from a member) | at the next statement, always |

**Config and metrics.** `[cache] guild_shape_ttl_secs = 60` (`SR_CACHE__GUILD_SHAPE_TTL_SECS`). A value of `0` turns the decorator into a pass-through. Two metrics: `sr_guild_shape_cache_total{kind,outcome}` and `sr_guild_shape_invalidations_total{source}`. The load test records the hit ratio.

**Phase 2**, only if phase 1 still misses:

- a `ChannelShape` projection (id, guild, kind, parent, nsfw, flags, thread private/archived/locked), used only by `dispatch_audience` and `can_view_channel`;
- `GET /users/@me/guilds` reading roles from the cache.

**Not chosen:**

- **TTL only:** a revoked overwrite would keep granting access for up to the TTL.
- **Per-request memoisation only:** `send_resolved` already does it.
- **Rows carried in `MessageCreated`:** that ties the event's shape to a node-local optimisation.
- **Caching member rows:** these are the only rows where staleness is a security defect.

## Tests that must exist

1. **sr-db unit tests** with a mock inner repo:
   - a repeated read is a hit;
   - `bump` makes the next read a miss;
   - a bump between the SELECT and the insert caches nothing;
   - a short TTL expires;
   - `ttl = 0` never caches.
2. **A Postgres test per writer** listed above. Set the TTL to one hour, warm the cache, write, and assert that the very next read returns the new value.
3. **End-to-end revocation**, with the cache primed and the TTL at one hour. After each of these, the next `POST` must answer 403 and the member must be absent from `dispatch_audience`:
   - a role overwrite denying `VIEW_CHANNEL`
   - a member overwrite
   - `RoleUpdated` removing `SEND_MESSAGES`
   - role deleted
   - `ADMINISTRATOR` removed
   - ownership transferred away

   Then the never-cached cases, primed, to show they need no invalidation: kick, ban, timeout, a role removed from the member, and leaving a private thread.
4. **Gateway:** `dispatch_audience_matches_the_per_user_checks` runs primed, revokes, and checks again. The `lists.rs` member list stops showing a channel after an overwrite change.
5. **Bus ordering:** publishing `RoleUpdated` on `LagBus` bumps the generation before any subscriber receives it. A firehose-delivered event bumps too.
6. **Statement count:** a warm second message costs at most 10 statements, and 16 again with `ttl = 0`.
7. **Memory bound:** 5,000 guilds evict down to the configured capacity.
