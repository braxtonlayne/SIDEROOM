# Handoff: feat/p35b-sync-perms (BRG-SYN-002)

Wave 35b, the whole-guild Discord mirror: permissions. Base `integration/phase35b` (a98c3540). Not pushed. Built
against the planned sync-core types (no commits from the other three agents existed when this was written), so the
modules here are pure over `sr-core` types and one table of their own; nothing needs `SyncOp`, `SrStructuralPatch`
or `SyncedState` to compile.

## Done

| Piece | Where |
|---|---|
| Bit masks `Permissions::BRIDGE_SHARED` (0-46, 48-52), `Permissions::SR_ONLY` (56-62 as defined), `DISCORD_ONLY_MASK` (47, 53-55), `ClassifiedBits::{from_discord,to_discord}`; exhaustive test `unclassified_bits_are_none` (a new bit fails the build until classified) | `crates/sr-core/src/permissions.rs` |
| Audit actions 220 `BridgeModeUpdate`, 221 `BridgeSyncRun`, 222 `BridgePermissionReverted`, 223 `BridgeConflict`, 224 `BridgeObjectRemoved` | `crates/sr-chat/src/settings/audit.rs` |
| Migration **1754** `bridge_conflicts` (1755 left unused) | `crates/sr-db/migrations/1754_bridge_conflicts.sql` |
| Role translation: push word, drift, role created from Discord (no permissions, position 1, proposal kept), owner-confirmed import (+ red `DANGEROUS` set), role-deletion guard (a role that denies anything in an overwrite is not deleted), role order (SIDE ROOM's, a Discord reorder is detected) | `crates/sr-bridge/src/sync/perms.rs` |
| Overwrite translation `plan_push`: `@everyone` by identity, SR-only bits never sent, SR member overwrites never sent, Discord member / managed-role allow bits trimmed and denies kept, bot's own overwrites untouched, Discord-only bits preserved, reverts reported; `inherit_from_parent` | `crates/sr-bridge/src/sync/overwrites.rs` |
| Conflict log: record with 10-minute coalescing (one audit row per new conflict), list, get, resolve once, supersede, prune closed rows after 90 days; `write_audit` helper for 220/221/224 (sync-core calls it) | `crates/sr-bridge/src/sync/conflicts.rs` |
| Routes (class `bridge_admin`): `GET /guilds/{g}/bridge/conflicts`, `GET .../conflicts/{conflict_id}`, `POST .../dismiss`, `POST .../resolve` ("Apply on SIDE ROOM", needs MANAGE_ROLES or MANAGE_CHANNELS), `POST .../take-discord` (structure conflicts: role name/color/hoist/mentionable, channel name/topic, through the normal edit services as the clicking person; never a permission) | `crates/sr-api/src/routes/guilds/bridge_conflicts.rs` |
| `route_matrix.tsv` rows (5, reviewed, no `?`); `authz_matrix.rs`: a seeded open conflict per tenant (`{conflict_id}`) and a `{}` body for take-discord | `crates/sr-api/tests/route_matrix.tsv`, `crates/sr-server/tests/authz_matrix.rs` |
| Authz property tests (design 16.3) | `crates/sr-bridge/tests/authz_property.rs` |
| IMMUTABLE check for the bridge keys | `crates/sr-server/tests/immutable_rules.rs` (`the_whole_guild_bridge_adds_no_switch_for_the_three`) |

## The guarantee and its proof

`sync` takes SIDE ROOM state by shared reference and returns what to send or what to add. The property test
(`no_discord_event_raises_any_sideroom_permission`) runs random Discord events (role words incl. `u64::MAX`,
`@everyone`, overwrites for roles, members, the bot and managed roles, reorders, role deletes, role and channel
creates in private categories, ADMINISTRATOR) against a model SIDE ROOM guild with the real planners, after every
event, and asserts: no member's computed permission in any pre-existing channel grows, no role word / `@everyone`
word / surviving overwrite allow-set changes, relative role order is unchanged, no word sent to Discord carries
bits 56-63, and Discord's shared bits equal SIDE ROOM's. Plus: Discord-only bits survive passes, SR member
overwrites never reach Discord, a "Muted" role is recreated not deleted. 500 cases default, `SR_PROP_CASES=10000`
(ran green once locally). The test found one real hole while being written: the trim path echoed bit 56 back
from a hostile Discord overwrite; `plan_push` now drops bits no Discord word may carry.

Not a compile-fail test: `SrStructuralPatch` is sync-core's type and does not exist yet (and `trybuild` is not in
the lock). Add a `compile_fail` doc test on it when it lands.

## Not done / for the integrator

- **`sync/mod.rs` conflict:** sync-core also owns `crates/sr-bridge/src/sync/mod.rs`; mine declares only
  `conflicts`, `overwrites`, `perms`. Union the module lines. `lib.rs` got one line `pub mod sync;`.
- **Route-matrix and authz rows for other agents' routes: none added** (their branches had no commits). Needed rows
  (design 16.3): `GET /guilds/{g}/bridge/plan/{plan_id}`, `POST /guilds/{g}/bridge/plan`, `PATCH .../plan/{plan_id}`,
  reconcile, pause/resume and `GET status` additions: `bridge_admin`; `POST /guilds/{g}/bridge/mode`:
  `guild_owner`. The matrix needs a seeded `{plan_id}` per tenant, which depends on sync-core's 1752
  `bridge_sync_plans` columns. Conflict actions needing sync-core machinery (Restore, Delete now, Unfreeze, Retry)
  are not routes yet; `Resolution` already has the values.
- **Callers:** nothing calls `plan_push`, `compare_role`, `record_conflict` yet: sync-core's reconciler and
  sync-gateway's dispatch handling do (conflict kinds, fields and value shapes are in the migration comment).
  The Discord actor lookup (audit-log match within 10 s) is the caller's.
- **Web:** `apps/web/src/settings/server/auditText.ts` needs texts for 220-224 (sync-ui). docs/09 retention: closed
  conflict rows 90 days, all rows go with the bridge on teardown; they hold a Discord user id and name.
- OpenAPI, api-client schema and MSW handlers regenerated for the 5 routes (`mocks:check` green). Regenerate again after the other agents add routes.
- Position handling for a Discord-created role: the model shifts existing roles up by one (as the repository
  does); the property is stated on relative order.

## Decisions relied on (design section 19 defaults)

Q1 (role order SIDE ROOM only), Q2 (guarded role deletion), Q3 (Discord member/managed overwrites: allows trimmed,
denies kept), Q4 (import of Discord-only roles on owner confirmation, dangerous bits red), Q6 (new Discord channel
takes the SR parent's overwrites). No new settings keys were added by this item.

## Owner questions

None new. Q1-Q4 and Q6 above are the owner's to change; Q3 and Q1 are where "ceilings only shrink" is enforced.

## Tests run (scratch DB `sr_p35b_syncperms`, `-j 2`, one process at a time)

- `cargo test -p sr-core --lib permissions`: 19 pass (5 new mask tests).
- `cargo test -p sr-bridge --lib sync`: 16 pass. `--test authz_property`: 5 pass (500 cases, and once with
  `SR_PROP_CASES=10000`). `--test conflicts` (Postgres, `--include-ignored --test-threads=1`): 5 pass.
- `cargo test -p sr-api --test route_matrix`: 6 pass; `--test openapi_names`: 2 pass; lib `bridge_conflicts`: 1 pass
  (audit numbers equal `AuditAction`).
- `cargo test -p sr-server --test immutable_rules -- --include-ignored --test-threads=1`: 14 pass (includes the
  matrix groups and the new bridge check); `--test authz_matrix -- --include-ignored`: 7 pass incl.
  `every_route_enforces_its_authz_row` (no ceiling or `WRITE_OBSERVED` entry changed).
- `cargo clippy -D warnings --all-targets` on sr-core, sr-chat, sr-bridge, sr-api; `cargo fmt --all --check`: clean.
- Not run: sr-chat/sr-db suites beyond compilation (only an enum variant and a migration were added), the web.
- Scratch DB `sr_p35b_syncperms` and templates are left for the integrator to drop; no server was started.
