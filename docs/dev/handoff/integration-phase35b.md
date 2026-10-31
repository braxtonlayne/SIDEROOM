# Integration: phase 35b (the whole-server Discord mirror, BRG-SYN-001 to 004)

Branch `integration/phase35b` (base `integration/phase35` a98c3540, fast-forwarded to `main` 5b242383 = PR #66).
Merged `--no-ff`: `feat/p35b-sync-core` (78f3b09d), `feat/p35b-sync-gateway` (f34b19eb), `feat/p35b-sync-perms`
(18e21737), `feat/p35b-sync-ui` (b49e30a4). Not pushed; no PR opened. Real Discord was never contacted: every run
used the in-memory Discord of the Rust tests or `e2e/support/fake-discord.mjs` (REST, gateway and sync-gateway's
structural half) on 127.0.0.1:8090.

Design: `p35b-design.md` (commander). Decisions: D-386 and P35B-Q1 to Q11, P35B-UI-1 to 3, P35B-INT-1 and 2
(`docs/01-decisions-log.md`, section "Phase 35b").

## Merge notes

- sync-core had cherry-picked sync-gateway 27aaecd4 and sync-perms 492c46ef: the only conflicts were the gateway
  handoff (took the later file), `sr-bridge/src/sync/mod.rs` (union of the module lines: sync-core's engine modules
  plus `conflicts`, `overwrites`, `perms`; `lib.rs` keeps one `pub mod sync;`) and the guilds router (both
  `bridge_sync` and `bridge_conflicts`). sync-ui merged clean.
- `Cargo.lock`: only the new dependency edges of `sr-bridge` (`sha2`, `unicode-normalization`, `proptest` as a
  dev-dependency); no new package or version entered the lock, nothing was downloaded.

## What the integration built (the gaps the four handoffs listed)

**Engine to permissions, conflicts and the gateway** (`crates/sr-bridge/src/sync/wiring.rs`, `reconcile_run.rs`,
`engine.rs`, `jobs.rs`):

- Every revert and conflict goes through `SyncEngine::note`: one `bridge_conflicts` row (sync-perms' coalescing,
  audit 222/223) with the Discord actor the `GUILD_AUDIT_LOG_ENTRY_CREATE` dispatch named for that object in the last
  10 seconds (`Attributions`). Kinds now written: `permission_reverted` (role words, overwrites, role order),
  `overwrite_trimmed`, `structure_conflict`, `role_deletion_refused`, `deletion_held`, `dead_op`, `frozen`. A
  Discord edit is told from a SIDE ROOM edit going out by the last pushed value (`synced_state.permissions`,
  `.overwrites`, `@everyone`'s `.role_order`), P35B-INT-1.
- **Ping-pong freeze** (design 11.4): more than five overrides of one object inside the ten-minute window freeze its
  map (`frozen` row; the counting row closes with it). "Unfreeze" (`POST .../conflicts/{id}/unfreeze`) puts the map
  back and queues a look.
- **Overwrites** go through sync-perms' `overwrites::plan_push` on every pass for every mapped channel (threads
  excepted) and as the seed's `OverwritePush` hook (`PlannedOverwrites`, attached in `app.rs`): shared bits only,
  Discord-only bits kept, SIDE ROOM member overwrites never sent, Discord member and managed-role allows trimmed, the
  bot's own member and role untouched.
- **Role order** is SIDE ROOM's on every pass (Q1): a Discord reorder is put back and logged; a SIDE ROOM reorder is
  pushed.
- **Gateway** (`EngineSink`, the `StructureSink` of sync-gateway): attached where `Inbound::new` is built
  (`bridge_inbound.rs`); a structural dispatch of a whole-guild bridge whose sync runs queues one look (debounced, one
  pending op per object). **SIDE ROOM changes** (role, channel, thread events on the bus) queue looks the same way
  (`spawn_sync_listener`).
- **Mode change restarts the shard** (`SyncEngine::with_mode_hook`: the manager stops the bridge's shard; the
  supervisor starts it with the new mode's intents).
- **Mentions**: `Names.role_ids` / `channel_ids` (outbound) and `MentionMaps` (inbound create and edit) are loaded
  from the role and channel maps in whole-guild mode (`sync::mention_maps`); selective bridges keep wave 35's text.
- **Members**: `GET .../bridge/members` carries `role_ids` (display only, from `bridge_members.discord_role_ids`
  through the role maps) in whole-guild mode. Channel `sr_bridge` (REST and gateway) carries `mode`.
- **Breakers**: the invalid-request counter is shared by every clone of `DiscordApi` and keyed per bot token (a one-way
  key, never the token): a bridge pauses at 200 in ten minutes, every bridge at 2,000 (`INSTANCE_INVALID_LIMIT`).
- A seed cut short by a restart resumes on start; closed conflict rows are pruned after 90 days and lapsed plans a
  day after they expired (doc 09).

**One contract between the screens and the server** (`apps/web/src/settings/server/bridge/api.ts`, wire types
`Wire*` turned into the view models there; the MSW handlers of `sync.test.tsx` answer the wire shapes):

| UI expectation (sync-ui) | Decision |
|---|---|
| `GET sync` vs `status` | `GET .../bridge/sync` (sync-core's name) is the sync status; `GET .../bridge/status` stays wave 35's |
| `GET preflight` | built server-side (`bridge_sync.rs::preflight`, `bridge_admin`): the plan's facts and blockers without storing a plan |
| `GET mirrors` | built server-side (`membership`): role and channel maps with state, Discord-only bit numbers, degraded reason |
| `GET teardown-preview` | sync-core's route (`guild_owner` + MFA); the UI calls it through the re-auth wrapper |
| `PATCH bridge.enabled {teardown}` | dropped. Options go to `POST .../bridge/teardown` (`guild_owner` + MFA), then the module switch; the dialog shows the teardown's report (maps, leftover webhooks, `failed_ids` as residue) |
| plan rows, link, unlink, import | rows are the plan's items addressed by index; link sends `{kind, sr_id, discord_id}`; not-mirrored reasons carry a stable `code` |
| conflict actions | `dismiss`, `resolve`, `take-discord` (sync-perms), `unfreeze`, `retry` (new); "Restore" and "Delete now" are the holds routes, which now close the `deletion_held` row |
| sync status fields | `bot` checks derived from `paused_discord_access` (the minute check pauses the moment either is lost) and `intents_ok`; `reconcile_available_at`; queue = pending + running; one-sided = held deletions |

**Matrices**: `route_matrix.tsv` rows for every 35b route (G-19 classes, no `?`): `bridge_admin` for plan, sync,
reconcile, pause, resume, holds (+ `MANAGE_CHANNELS` in the handler), preflight, conflicts incl. unfreeze and retry;
`guild_owner` for mode, teardown-preview, teardown; `membership` for mirrors. `authz_matrix.rs` seeds per tenant an
open plan (`{plan_id}`) and two held channels (dedicated ones for "Delete now" and "Restore", path overrides), bodies
for plan, plan edit, mode and teardown, and runs the teardown late (it removes the bridge every other bridge route
addresses). No ceiling moved: `SHAPE_CEILING`, `WEAK_CEILING`, `WRITE_OBSERVED_LEN` unchanged.

**Web**: audit texts 220 to 224 (`auditText.ts`), fragment `i18n/messages/en/p35b-integ-1.ts`.

## Gates (merged tree, this machine)

| Gate | Result |
|---|---|
| `cargo fmt --all --check`; `clippy --all-targets -D warnings` on sr-core, sr-chat, sr-bridge, sr-api, sr-admin, sr-db, sr-server | pass |
| `sr-bridge` (Postgres, `--include-ignored --test-threads=1`): lib 81, `authz_property` 5 (500 cases), `conflicts` 5, `db` 16, `inbound` 15, `structure` 8, `sync_db` 9, `sync_engine` 24 (incl. 4 integration wiring tests), doc 3 (incl. the `compile_fail` proofs that `SrStructuralPatch` has no permission or position field) | pass |
| `sr-core --lib permissions` 19; `sr-chat --lib` 386; `sr-admin` 68; `sr-db` `bridge_dsr`, `migration_versions`, `data_export`, `account_deletion`, `message_store` | pass |
| `sr-api` lib 794, `route_matrix` 6, `openapi_names` 2 (no clashes) | pass |
| `sr-server` lib 238, `immutable_rules` 14 (incl. the bridge check: no switch for E2EE, authz or the logs; new keys pass the deny pattern), `web_gate` 19 | pass |
| `sr-server --test authz_matrix` 7 incl. `every_route_enforces_its_authz_row` (all 35b routes, seeded `{plan_id}` and held channels, cross-tenant sweep), `storage_scopes_hold_over_http`, `write_observed_is_pinned`; SHAPE_CEILING, WEAK_CEILING, WRITE_OBSERVED_LEN unchanged | pass |
| OpenAPI regenerated (`cargo run -p sr-server -- openapi`), api-client generated, MSW handlers generated, `mocks:check` | pass (1143 operations) |
| `node scripts/lib/gateway-docs.mjs`, `check-doc-links.mjs` | no change / ok |
| Web: typecheck (web and e2e), eslint (web, api-client), prettier | pass |
| Vitest `apps/web` `--maxWorkers=2` | 402 files, 3,688 tests pass |
| `build-web`, `check-entrance-isolation` (D-385), `check-bundle-budget` | pass |
| Playwright Chromium, dev mode (sr-server 8080 with `memory:` storage, scratch DB, fake Discord on 8090, Vite 5173): `bridge.spec.ts` and `bridge-whole-guild.spec.ts` | 2 pass (the whole-guild spec: plan and preflight against the fake, confirm, seed, a Discord permission edit put back and logged with "Apply on SIDE ROOM" saving nothing, rename from Discord arrives, deletion held and restored, back to selective deletes nothing, whole again and switch off with "also remove"). First runs failed on the spec's locators and on a leftover whole-guild bridge of an earlier failed run (the preflight's unique-server check, correctly) |

## Not done (and why)

1. **Hiding a held channel.** A channel deleted on Discord is held (row, conflict, "Restore" and "Delete now" work, the
   hold job deletes it after `bridge.delete_hold_hours`), but the channel list, the gateway and the read routes still
   show it during the hold (design 6.4 wants it hidden at once). It touches every channel read path in `sr-chat` and
   the gateway's audience; too wide for the integration. The e2e spec asserts the hold through the conflict log.
2. **`Origin::Bridge` in `sr-chat`** (P35B-INT-2): changes applied here from Discord run as the server owner, so their
   ordinary audit rows name the owner, not the bridge user with `options.sr_bridge`. Loop prevention rests on the
   `synced_state` comparison (design 11.3 layer 2). A shared parameter on the role, channel and thread services.
3. **Thread state, forum posts and tags, channel order after the seed**: the reconcile compares role and channel
   fields, permissions, overwrites and role order; it does not compare thread archived/locked state, forum tags or
   channel positions after the seed, and `SrStructuralPatch::Thread` is not applied by `ChatSrStructure`.
4. **The plan build is synchronous** inside `POST .../plan` (seconds for a small server), not a queued job.
5. **Seed progress and the rate-limit wait** are not reported by `GET .../sync` (the status page shows no progress bar
   and "None" for the wait). The "other bot edited roles in the last hour" preflight warning is not built.
6. **Playwright on Firefox and WebKit** (Firefox does not launch on this machine; CI runs the three engines).
7. **Real Discord**: the first whole-server run against a real Discord server is the owner's (design R3).

## Owner items

- D-386 and P35B-Q1 to Q11 are defaults pending the owner (role order and permissions SIDE ROOM's, guarded role
  deletion, member-overwrite trims, owner-confirmed import with Administrator, no exclusions, no backfill, voice
  structure only, teardown leaves Discord as it is). P35B-UI-1 to 3 are the commander's answers to sync-ui's questions
  (Administrator warning kept; resolved conflicts kept 90 days; the way back greyed for managers).
- P35B-INT-2 (`Origin::Bridge`) and the held-channel hiding are the next wave's if the owner wants them.

## Clean-up

Scratch databases `sr_p35b_integ` and `sr_p35b_e2e` dropped, with the template databases this run created;
sync-perms' leftover `sr_p35b_syncperms` and the 35b templates `sr_tpl_195_*` dropped (un-templated first). The
servers this run started (sr-server on 8080, the fake on 8090) were stopped by PID. The integration target directory
is `p35b-integ-target` (sync-core's build cache, moved).
