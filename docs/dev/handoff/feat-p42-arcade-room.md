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

All acceptance gates ran green on this branch. Nothing is blocked.

| Gate | Result |
|---|---|
| sr-core tips, sr-activities lib, sr-activities Postgres (arcade 6, db 9), sr-packages profile_doc | ok |
| route_matrix (6), openapi_names (2) | ok (rows: admin staff+OPERATOR, cartridges membership, notify self, /arcade user, bot deny) |
| authz_matrix (all 8, seeds for entry, cartridges, released game added) | ok |
| immutable_rules (22, ignored run) | ok |
| sr-db secret_columns, migration_versions | ok |
| OpenAPI, api-client generate, mocks:check (1157 ops) | regenerated, up to date |
| cargo fmt --check, clippy -D warnings --all-targets (sr-core, sr-activities, sr-packages, sr-api, sr-server) | ok |
| web tsc, eslint (touched dirs, profile-blocks), prettier --check ., Vitest apps/web 424 files / 3846 tests, profile-blocks 7 | ok |
| build-web, check-entrance-isolation, check-bundle-budget | ok |
| e2e tsc (arcade-room.spec.ts) | ok; Playwright NOT run (wave-41 integrator owns the ports) |

Not done / for later: Notify me delivery (ArcadeService::watchers), QR for profile tip links (rendered as payment link + text), guild cartridge launch from the shelf (needs wave 41 shelf API), Bitcoin checksum verification. The wasm build (pnpm --filter @sideroom/wasm build) is needed before the full web suite.

## Notes for the wave-42 integrator

- w41 `RoomsNav` links the Arcade to `/games` gated by `useEmulatorOn()`; the Arcade must show regardless of the emulator flag.
- w41 also edits `CAPABILITIES` in `bundle.rs`; this branch accepts `byos_read`/`byos_write` through `arcade::FLAGS`, so both merge cleanly.
- arcade-sandbox (42-B) plugs trust badges in through the card seam (to be added in step 4); cards key on `bundle_sha256` of the reviewed zip, not the `code_sha256` of 42-B.
- Notify me stores opt-ins only; delivery of the release notice is open (`ArcadeService::watchers`).

## Open owner questions

- Guild cartridge `source_ref` is an opaque shelf locator until the wave-41 shelf API is merged.
- Bitcoin address checksums are not verified server side.
