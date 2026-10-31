# feat/p35b-sync-core (BRG-SYN-001)

Branch `feat/p35b-sync-core` (base integration/phase35b a98c3540). Not pushed. Real Discord never contacted (an
in-memory Discord guild in `crates/sr-bridge/tests/sync_engine.rs`). Scratch DB `sr_p35b_synccore` (dropped at the end).

## Cherry-picked from colleagues
- `27aaecd4` (sync-gateway): `discord_structure.rs`, the `DiscordApi` structure calls. Every Discord call of the engine goes through it.
- `492c46ef` (sync-perms): `Permissions::BRIDGE_SHARED` / `SR_ONLY` / `DISCORD_ONLY_MASK`, audit actions 220 to 224. The engine uses `BRIDGE_SHARED` for every word it sends. I did **not** take sync-perms's `sync::{perms, overwrites, conflicts}` (their `sync/mod.rs` and `Cargo.toml` edits collide with mine); the engine reaches them through the `OverwritePush` hook and writes no conflict rows (see open).

## Done
- **Migrations 1750 to 1753.** 1750 `bridges.sync_state`, mode audit fields, `last_reconcile_at`, unique whole_guild index per Discord server; map columns `selected`, `origin`, `parent_map_id`, `synced_state`, `sync_status`, `degraded_reason` (`bridges.mode` and `bridge_channel_maps.kind` already existed in 1720, and its webhook check already allows NULL pairs, so nothing was relaxed). 1751 `bridge_role_maps`. 1752 `bridge_sync_ops`, `bridge_sync_plans`. 1753 `bridge_pending_deletes`, `bridge_discord_snapshot`.
- **`crates/sr-bridge/src/sync/`**
  - `types`, `patch` (`SrStructuralPatch`: no permission, overwrite or role-position field; three `compile_fail` doctests), `reconcile` (authority table, pure three-way `decide`).
  - `eligible::bridgeable` (the one predicate; `refusal_for_channel` now also allows forum/media for explicit picks).
  - `store`, `ops` (one pending row per object, 1 s / 2 s debounce, priority, claim with `SKIP LOCKED`, backoff 1 s to 10 min, dead after 8, retry, crash recovery), `holds`.
  - `plan` (matching of design 9.3: pinned, `@everyone`, roles, categories, channels by family and parent, threads; duplicates in position order; NFKC names; diff edits unlink/link/import; blockers; SHA-256 hashes; `DANGEROUS_BITS`).
  - `engine::SyncEngine`: `create_plan` (supersedes the open plan, 15 min), `get_plan`, `edit_plan`, `confirm_mode` (hash and both sides' state re-checked, `PlanStale` otherwise, audit 220), `switch_to_selective` (deletes no object anywhere), `run_seed` (snapshot first, roles, role order, categories/channels/threads, positions by seed structure, overwrite hook, role permission words, live, audit 221; idempotent resume), `hold_deleted_on_discord`, `run_due_holds`, `restore_held`, `delete_held_now`.
  - `reconcile_run::reconcile_guild`: three-way for role (name, colour, hoist, mentionable) and channel (name, topic, nsfw, slowmode) fields; SIDE ROOM wins when both changed; **role permission words on Discord are put back to SIDE ROOM's shared bits | Discord-only bits** (revert); roles/channels that exist on one side only are created on the other (a role from Discord has no permissions here and keeps only its Discord-only bits there); a role deleted on Discord is recreated if it denies something in a channel here, else deleted here; a channel deleted on Discord is held.
  - `jobs`: ops runner (one reconcile per bridge per pass), scheduled reconcile, "reconcile now" (5 min gap), pause/resume, invalid-request breaker (200 in 10 min), bot-access check (`paused_discord_access`).
  - `teardown_opts` + `teardown.rs`: `TeardownOptions {remove_created, restore_permissions}` (default leaves Discord as is; only `origin = 'sr'` objects are ever deleted by the bot; failures become residue ids; audit 224), `teardown_preview` counts.
- **Routes** `crates/sr-api/src/routes/guilds/bridge_sync.rs` (session-only), `PlanStale` mapped to 409 `BRIDGE_PLAN_STALE`. `bridge.rs` helpers made `pub(super)` (same lines sync-perms changed).
- **Server**: `sr-server/src/bridge_sync_wiring.rs` (`ChatSrStructure` over the guild backend, `spawn_sync_worker`), `SyncEngine` extension and worker in `app.rs`; settings `bridge.delete_hold_hours` (0 to 168, 24), `bridge.reconcile_minutes` (5 to 60, 15), `bridge.struct_writes_per_sec` (1 to 10, 2) in `sr-admin`.
- New deps for sr-bridge: `sha2`, `unicode-normalization` (both already in the lock, used by sr-chat; nothing downloaded).

## New routes and the AUTHZ class for sync-perms's matrix rows (none are in route_matrix.tsv / authz_matrix.rs yet)
| Route | Class |
|---|---|
| POST `/guilds/{g}/bridge/plan`, GET and PATCH `/guilds/{g}/bridge/plan/{plan_id}` | `bridge_admin` (a plan id of another server is 404) |
| GET `/guilds/{g}/bridge/sync`, POST `.../bridge/reconcile`, `.../bridge/pause`, `.../bridge/resume` | `bridge_admin` |
| GET `/guilds/{g}/bridge/holds` | `bridge_admin` |
| POST `/guilds/{g}/bridge/holds/{channel_id}/restore` and `.../delete` | `bridge_admin` and additionally `MANAGE_CHANNELS` (403 without; another server's channel id is 404) |
| POST `/guilds/{g}/bridge/mode` | `guild_owner` + MFA |
| GET `/guilds/{g}/bridge/teardown-preview`, POST `/guilds/{g}/bridge/teardown` | `guild_owner` + MFA |

OpenAPI, the api-client and MSW mocks are **not regenerated** (four agents touch them; the integrator regenerates once). `openapi_names` passes (I renamed `ModeBody` etc. to `Bridge*`).

## Tests run (one cargo command at a time, `-j 2`)
`cargo test -p sr-bridge -- --include-ignored --test-threads=1` on `sr_p35b_synccore`: lib 58, `db` 16 (wave 35, unchanged), `inbound` 14, `sync_db` 9, `sync_engine` 20, doc 4. `sr-api --test openapi_names` 2, `sr-server --test immutable_rules` 13 (new settings keys pass the deny pattern). Clippy `-D warnings`: sr-bridge (all targets), sr-api, sr-admin (all targets), sr-server, sr-core, sr-chat (lib). `cargo fmt --all --check` clean.
`sync_engine` covers: plan is a dry run (no Discord write), supersede, cross-server 404, non-admin bot blocker, stale plan 409 and expiry, plan edits change the hash, seed both ways with SIDE ROOM-only bits never in any request body, owner-confirmed import, Discord-only bit preserved, resume without duplicates, degrade without Community, back to selective deletes nothing, hold/restore/delete, teardown defaults/remove/restore, reconcile (two-way fields, SIDE ROOM wins, Discord permission edit reverted and never reaches SIDE ROOM, creates both ways, guarded role deletion, held channel), ops runner/backoff, pause/resume/breakers.

## Not done / open (and why)
1. **`Origin::Bridge` in sr-chat.** Not added (a shared service parameter across many call sites). `ChatSrStructure` performs Discord-originated changes as the server owner (audit rows name the owner, not the bridge user with `options.sr_bridge`; design 13). Loop prevention relies on the `synced_state` comparison (design 11.3 layer 2, sufficient alone). The role creation position (design: directly above `@everyone`) is the backend's default, not forced.
2. **Not run against a real `sr-server` / HTTP.** The routes compile and are clippy clean but no HTTP-level test of them exists; the matrix rows (sync-perms) will exercise them. The authz matrix and `route_matrix` will fail until those rows exist.
3. **Not wired:** the gateway dispatch to ops (sync-gateway's `inbound/structure.rs`: events only call `enqueue_look`; until then the 15-minute scheduled reconcile and "reconcile now" are what notice a Discord change); the outbound listener for SIDE ROOM edits (same: `enqueue_look` is the entry; the scheduled pass covers it); `GUILD_AUDIT_LOG_ENTRY_CREATE` actor naming (the `discord_actor_id` column exists and is passed through).
4. **Conflict rows / proposals / audit 222 and 223** (permission reverted, structure conflict, frozen, refused role deletion, dead op) and the **ping-pong freeze** (5 changes in 10 min): sync-perms's `sync::conflicts` owns them; the engine counts conflicts and reverts in `ReconcileReport` and the audit 221 row but writes no `bridge_conflicts` row. Wiring is one call per `Decision::Revert/Conflict`.
5. **Overwrite pushes** go through the `OverwritePush` hook (default no-op); sync-perms's `sync::overwrites` is to be plugged in `app.rs`. Role **order after the seed** is pushed by the seed only (a later reorder on either side is not reconciled yet: `RoleOrder` ops are accepted but the pass does not compare order); channel order likewise outside the seed.
6. **Threads**: seeded (plan, create both ways), but the reconcile pass does not compare thread state (archived, locked, tags) and forum posts are not created on Discord by the seed (they ride the first message through the forum webhook); `ThreadPatch` is not applied by `ChatSrStructure`. Forum tags are not synced.
7. The plan build is synchronous inside `POST .../plan` (seconds for a small guild), not a queued job; the seed is a `tokio::spawn` after confirm, resumable by calling `run_seed` again, but a bridge left in `seeding` by a restart is not re-run automatically (the worker does not call it yet).
8. The "other bot edited roles in the last hour" warning of the preflight is not built; intents per mode are sync-gateway's.

## Decisions / owner questions
Q1 to Q11 defaults assumed. Relied on: Q1 (role order and permissions SIDE ROOM's; encoded in `field_authority`, unknown fields default to SIDE ROOM's), Q2 (guarded role deletion), Q4 (import with dangerous bits flagged, owner confirms; recorded as the owner's act), Q6 (new Discord channel takes the SIDE ROOM parent's overwrites via the port), Q7 (active threads only, no history), Q10 (the bot never deletes a Discord object it did not create; no option for it). Decisions taken here: name comparison uses NFKC + casefold and Discord's text-channel rule for channel names; teardown failures reuse `bridge_teardown_residue` (object ids in `discord_webhook_id`, the reason says `role`/`channel`).
