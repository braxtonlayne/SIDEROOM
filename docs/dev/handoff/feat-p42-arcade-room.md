# feat/p42-arcade-room (ARC-ROOM-001, wave 42-A)

Base `integration/phase42` (main 57b91ca9). Migration 2040. Cargo target `p42-arcade-room-target`; scratch DB `sr_p42_arcade_room`.

## Done (committed)

- `crates/sr-core/src/tips.rs`: tip link validator (btc, lightning, kofi, patreon; host allow-list `ko-fi.com`, `patreon.com`). 3 unit tests pass.
- `crates/sr-activities/src/arcade.rs`: `Tier` (exactly `TIER_1_ORIGINAL`, `TIER_2_REIMPLEMENTATION`; anything else rejected), per-tier `Eligibility`, capability flag catalogue (`datachannel`, `byos_read` active; `byos_write`, `guest_play`, `lobby_channel`, `ingame_voice`, `external_launch`, `asset_cache` reserved, accepted by the validator but never handed to an instance), flag-to-tier rules (`guest_play` needs Tier 1, `external_launch` needs Tier 2), manifest parser (`tier`, `tip_links`), `ArcadeService` (Featured, candidates, Coming Soon with Notify toggle, Guild Cartridges). 26 unit tests pass.
- `bundle.rs` now stores `tier` and `tip_links` on the bundle; Tier 2 launch in the browser is refused (desktop only); `InstanceView.capabilities` keeps only active flags.
- Migration `2040_arcade_room.sql` (bundle columns, `arcade_featured`, `arcade_coming_soon`, `arcade_coming_soon_notify`, `arcade_guild_cartridges`, CHECKs allow only the two tiers).
- Postgres tests `crates/sr-activities/tests/arcade.rs` (6) plus existing `db.rs` (9): all pass.
- `crates/sr-api/src/routes/arcade.rs`: 10 routes (`GET /arcade`, `PUT /arcade/coming-soon/{entry_id}/notify`, guild cartridges GET/POST/DELETE, `/admin/arcade/*` OPERATOR + audited: candidates, featured PUT/DELETE, coming-soon POST/DELETE). Registered in `routes/mod.rs`, `sr-server/src/app.rs`, bot-denied in `auth/policy.rs`. `cargo check -p sr-api` clean.
- Web helpers: `apps/web/src/arcade/{tips,types,api,adminApi}.ts`.

## Resume here

1. `SR_UPDATE_ROUTE_MATRIX=1 cargo test -j 2 -p sr-api --test route_matrix` (slow compile, run in background), review rows: GET /arcade `user`; notify `user`... (use `self`/`user`); guild cartridges `membership`; `/admin/arcade/*` `staff` with PLATFORM `OPERATOR`; BOT `deny`. Then `openapi_names` test (types use `schema(as = Arcade...)`).
2. Run `authz_matrix` (`every_route_enforces_its_authz_row`, ignored, scratch DB) and fix any seed ids / body overrides for the new routes (`application_id`, `entry_id`, `cartridge_id`).
3. Regenerate OpenAPI + api-client (`cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, `pnpm --filter @sideroom/api-client generate`, mocks).
4. Web UI not written yet: `TipBadges.tsx` (QRCode from `@sideroom/ui`, catch RangeError, ecc L), trust badge seam context for arcade-sandbox (`badges(card)` + `actions(card)`), `GameCard`, `ArcadePage` (Featured with static Emulator card, Guild Cartridges, Coming Soon with Notify), route: `/games` becomes the Arcade and the emulator moves to `/games/emulator` (update `emu/routes.tsx`, `cspProbe.ts`, `EmuPage.tsx` reload, `help/Handbook.tsx`, `emu/__tests__/emuPage.test.tsx`, `e2e/tests/emulator.spec.ts`), Admin section "Arcade" (OPERATOR) for curation, profile Linktree `tip` kind (server `profile_doc.rs` LinktreeCfg + web Inspector/renderer), i18n fragment `apps/web/src/i18n/messages/en/p42-arcade-room-1.ts`, css with logical properties.
5. Vitest: Arcade sections, tier validator (third value rejected, mirror of `TIERS`), tip host validation (`tips.ts`). Playwright spec `e2e/tests/arcade.spec.ts` written and typechecked, NOT run.
6. Gates: cargo fmt/clippy `-D warnings` on sr-core, sr-activities, sr-api, sr-server; `cargo test -p sr-db --test secret_columns --test migration_versions`; web typecheck/lint/prettier/vitest (`--maxWorkers=2`)/build + bundle budget.

## Notes for the wave-42 integrator

- w41 `RoomsNav` links the Arcade to `/games` gated by `useEmulatorOn()`; the Arcade must show regardless of the emulator flag.
- w41 also edits `CAPABILITIES` in `bundle.rs`; this branch accepts `byos_read`/`byos_write` through `arcade::FLAGS`, so both merge cleanly.
- arcade-sandbox (42-B) plugs trust badges in through the card seam (to be added in step 4); cards key on `bundle_sha256` of the reviewed zip, not the `code_sha256` of 42-B.
- Notify me stores opt-ins only; delivery of the release notice is open (`ArcadeService::watchers`).

## Open owner questions

- Guild cartridge `source_ref` is an opaque shelf locator until the wave-41 shelf API is merged.
- Bitcoin address checksums are not verified server side.
