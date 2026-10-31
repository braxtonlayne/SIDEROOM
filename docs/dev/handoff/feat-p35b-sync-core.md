# feat/p35b-sync-core (BRG-SYN-001): first slice

Branch `feat/p35b-sync-core` (base integration/phase35b a98c3540). Not pushed. Real Discord never contacted.

## Done (commit fc1b2555, cherry-pickable)
- Migrations 1750 to 1753 (all four apply; the sr-bridge Postgres suite, 16 tests, passes on scratch DB `sr_p35b_synccore`, dropped afterwards).
  - 1750: `bridges.sync_state`, mode-change audit fields, `last_reconcile_at`, unique whole_guild index on `discord_guild_id`; map columns `selected`, `origin` (matched/sr/discord), `parent_map_id`, `synced_state`, `sync_status`, `degraded_reason`. Note: `bridges.mode` and `bridge_channel_maps.kind` already existed in 1720, and its webhook check already allows NULL pairs, so nothing was relaxed.
  - 1751 `bridge_role_maps`; 1752 `bridge_sync_ops`, `bridge_sync_plans`; 1753 `bridge_pending_deletes`, `bridge_discord_snapshot`.
- `crates/sr-bridge/src/sync/` (`pub mod sync` in lib.rs): `types.rs` (SyncMode, SyncState, ObjectKind, OpDirection, OpState, OpPriority, SyncOp, SyncedState/FieldState), `patch.rs` (SrStructuralPatch with no permission, overwrite or role-position field; three `compile_fail` doctests prove it), `reconcile.rs` (authority table per design 2.2, pure three-way `decide`, Discord name normalization).
- Tests: `cargo test -p sr-bridge --lib sync` 11 pass; `--doc` 4 pass; `--test db` 16 pass.

## Not done (all of the rest of the row)
`bridgeable()` replacing `refusal_for_channel`; model queries for the new tables; ops queue (enqueue/debounce/run/dead); plan build, matching and hash; seed job; mode switch; deletion hold and jobs; teardown options; `Origin::Bridge` in sr-chat; `bridge_sync.rs` routes; jobs.rs entries. No sync-gateway DiscordApi methods existed yet (`DiscordApi` is a struct, not a trait), so nothing calls Discord.

## Planned routes and intended AUTHZ class (for sync-perms; none added yet)
- GET /guilds/{g}/bridge/plan/{plan_id}, GET .../bridge/sync-status (status), POST .../bridge/plan, PATCH .../bridge/plan/{plan_id}, POST .../bridge/reconcile, POST .../bridge/pause, POST .../bridge/resume: `bridge_admin`.
- POST .../bridge/mode: `guild_owner` + MFA.
- Teardown options (extend the existing token DELETE body): `guild_owner`.

## Decisions / owner questions
Q1 to Q11 defaults assumed; the skeleton encodes Q1 (role order and permissions SIDE ROOM-authoritative) in `field_authority`; unclassified fields default to SIDE ROOM authority.
