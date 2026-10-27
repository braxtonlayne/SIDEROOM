# 21 — Feature phase (`BYOS`, and the later prefixes of roadmap-p27)

Waves 27 to 40. Plan, acceptance criteria, agent split and owner needs: [dev/roadmap-p27.md](../dev/roadmap-p27.md) (D-380). Prefixes are those of roadmap-p27 section 4; each wave's integrator adds its items here.

Same conventions as [20-launch-readiness.md](20-launch-readiness.md): an item ticks only when its proof runs (a CI job, a Postgres or browser test, or a recorded run in the integration handoff), never when it is only written. The live Garage cases of `storage.yml` run only on CI; an item ticked on its mock and Postgres proofs says so, and its CI run is linked once the owner pushes.

## Wave 27 — BYOS core

Integration: [dev/handoff/integration-phase27.md](../dev/handoff/integration-phase27.md).

### BYOS-MNT — Mounts
- [x] **BYOS-MNT-001** [DB][BE] Mounts and sealed credentials: migration 1400 `storage_mounts`, `sr_storage::mounts` (`MountRepo`, `MountResolver`, sealed credentials), `CachedMountResolver`, user and guild mount CRUD routes, `sr-cli` rotate for mounts, `sr_storage::path::safe_key` _Proven 2026-10-26 on the merged tree (integration-phase27.md): `cargo test -p sr-storage`, `sr-db --test mounts_repo` (Postgres), `sr-api --lib` (mount route tests), `route_matrix`._
- [x] **BYOS-MNT-002** [BE] Connection test and provider guidance: `POST .../mounts/{mount_id}/test` (pass/warn/fail rows, 20 s budget), `abuse.storage_tests_per_hour` _Proven 2026-10-26 on the merged tree (integration-phase27.md): `sr-storage --test s3_probe_mock`, `sr-api --lib` (rate limit and route tests). The Garage live case runs on CI only._
- [x] **BYOS-MNT-003** [BE][CLI] CORS checker and apply: `.../cors/check`, `.../cors/apply`, daily job `storage.cors_recheck` (id 134), `sr-cli storage ensure-cors` _Proven 2026-10-26 on the merged tree (integration-phase27.md): sr-storage CORS checker tests (mock S3), `sr-server --lib admin_jobs` (job id, kind and key unique), `cargo test -p sr-cli`. The Garage live case runs on CI only._

### BYOS-SCP — Scopes
- [x] **BYOS-SCP-001** [DB][BE] Scopes, shares and grants: migration 1410, `sr_storage::scopes::resolve`, Friend Shelf shares, guild storage grants, unfriend hook _Proven 2026-10-26 on the merged tree (integration-phase27.md): `sr-db --test scopes_repo --test social_repo` (Postgres), `authz_matrix storage_scopes_hold_over_http`._
- [x] **BYOS-SCP-002** [BE] Scoped listing and presign: objects listing, metadata, presign (TTL settings `byos.presign_*`), authz matrix storage actors _Proven 2026-10-27 on the merged tree (integration-phase27.md): `authz_matrix` `every_route_enforces_its_authz_row` (every group, group rest at 16 weak refusals and 45 malformed, no ceiling raised) and `storage_scopes_hold_over_http`, the browse tests in sr-storage._

### BYOS-INS — The instance bucket
- [x] **BYOS-INS-001** [DB][BE] The instance bucket as mount 1: migration 1420 (`mount_id`, `source`), `MountStorages`, mount 1 seeded at start, a changed bucket refused _Proven 2026-10-27 on the merged tree (integration-phase27.md): the full workspace `cargo test --workspace --exclude sr-storage -- --include-ignored --test-threads=1` on Postgres (3489 passed, 0 failed) with `instance_mount` and its Postgres test; the existing suites pass unchanged._

### BYOS-UI — Settings
- [x] **BYOS-UI-001** [FE] User Settings > Storage and Guild Settings > Storage, Friend Shelf toggles, role editor storage grants, bucket browser read side, nav modes, `check-css-tokens` _Proven 2026-10-26 on the merged tree (integration-phase27.md): vitest on src/settings, src/storage, src/lounge (146 files, 1162 tests), typecheck, eslint, prettier, i18n-audit 0, check-css-tokens, build and bundle budget._

## Wave 28 — BYOS write side, catalog, legal, flags and the harness

Integration: [dev/handoff/integration-phase28.md](../dev/handoff/integration-phase28.md). The Postgres password file was
missing for the whole integration, so no Postgres suite ran on the merged tree; every row below waits on at least one
Postgres proof and stays unticked until it runs (the handoff lists them in order).

### BYOS-BRW — The bucket browser, write side
- [ ] **BYOS-BRW-001** [BE][FE] Multipart upload and folders through the ledger: migration 1440 and 1441, `sr_storage::uploads`, upload, folder, move and delete routes, the `storage.uploads_sweep` job (id 138), the mount upload kind of the IndexedDB ledger, the upload panel _Merged tree: `sr-storage` (incl. `s3_multipart_mock`), `sr-api --lib` storage write tests, `route_matrix`, vitest src/storage. Waits on: `sr-db --test mount_uploads_repo --test mount_attachments`, `authz_matrix storage_writes_hold_over_http` and group rest (Postgres); the live `storage.yml` case (CI)._

### BYOS-CAT — Catalog and shelves
- [ ] **BYOS-CAT-001** [BE][JOB] Catalog indexing: migration 1450, `sr_storage::index`, `PgIndexStore`, the `storage.index` job (id 1450), delisted keys from the takedown register, the browser's delete and move and an upload's complete update the index _Merged tree: sr-storage index tests, `sr-server --lib storage_jobs` (204-name tree, 0 misfiled). Waits on: `sr-db --test index_repo` and `sr-server storage_index_pg_tests` (Postgres)._
- [ ] **BYOS-CAT-002** [FE] Shelves in Cinema and Library and the metadata editor: `GET|PATCH /shelves/...`, source chips, playback from a presigned URL, CBZ by range, `?guild=` with `member_shelves_in_guild` _Merged tree: sr-api shelves unit tests (incl. `member_shelves_in_guild_filters_the_server_section`), `route_matrix`, vitest src/storage, src/library, src/cinema. Waits on: `sr-db --test shelf_repo` and `authz_matrix shelves_hold_over_http` (Postgres); the `byos-mount.spec.ts` step (not written)._

### BYOS-LEG — Takedown and delisting
- [ ] **BYOS-LEG-001** [BE][FE][DOC] Takedown and delisting for members' buckets: item kind `storage_object` (migration 1460), whole-mount delist and relist, DSR export and erasure, Admin > Storage, lawyer brief Q20-Q23, threat-model rows T30 and T31; P28-LEG-1 (a server bucket strikes the uploader the ledger knows, else the owner) _Merged tree: `sr-moderation`, `sr-admin`, `sr-storage` unit tests, vitest src/admin and src/moderation. Waits on: `sr-moderation --test storage_takedown` (with the new P28-LEG-1 test) and `--test takedowns`, `sr-db --test storage_legal` (Postgres); the `moderation-gate` storage step (Playwright)._

### BYOS-FLG — Settings and the feature
- [ ] **BYOS-FLG-001** [BE][FE] `platform.byos`, `guild_mounts`, `member_shelves_in_guild`, the `byos.*` settings (one copy of each key) _Merged tree: `sr-features byos`, `sr-admin` settings catalog tests, `sr-storage mount_policy`, `sr-api --lib` storage (allow-list, who_may_mount), vitest src/settings. Waits on: the Postgres settings tests through the HTTP stack; the endpoint test button for staff and a full HTTP test that the kill switch keeps mount 1 reads were not built._

### OPS — Operations
- [ ] **OPS-IDX-001** [OPS][BE][FE][TEST] Only the landing page is indexable (D-384) _Merged tree: `sr-api --test api` (`every_response_says_noindex`), vitest `useNoindex` and `csp.test.ts`, threat-model row T29. Waits on: `e2e/tests/noindex.spec.ts` in the prod-smoke `stack` job (CI)._
- [ ] **OPS-IMM-001** [BE][FE][TEST] E2EE is the only voice mode _Merged tree: `sr-core` config tests (`e2ee = false` is an error), `sr-gateway`, `sr-voice` (loopback and e2e unkeyed-sender tests), vitest src/voice. Waits on: `sr-server voice_pg_tests` (Postgres) and the three-engine `voice-e2ee.spec.ts` (CI)._
- [ ] **OPS-IMM-002** [TEST] The IMMUTABLE RULE harness, LEGACY pinned by P28-OPS-1 _Merged tree: the non-Postgres tests of `immutable_rules.rs` (name, description and deny-pattern tests). Waits on: `immutable_rules refusals_and_logs_hold_in_every_flag_state` and the full `authz_matrix` (Postgres)._
