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
- [ ] **BYOS-SCP-002** [BE] Scoped listing and presign: objects listing, metadata, presign (TTL settings `byos.presign_*`), authz matrix storage actors _Open: `storage_scopes_hold_over_http` and the browse tests passed on the merged tree; `every_route_enforces_its_authz_row` (group 5) not yet run there._

### BYOS-INS — The instance bucket
- [ ] **BYOS-INS-001** [DB][BE] The instance bucket as mount 1: migration 1420 (`mount_id`, `source`), `MountStorages`, mount 1 seeded at start, a changed bucket refused _Open: `instance_mount` tests (incl. Postgres) passed on the merged tree; its proof is the unchanged existing suite, so it ticks after the full workspace `cargo test`._

### BYOS-UI — Settings
- [x] **BYOS-UI-001** [FE] User Settings > Storage and Guild Settings > Storage, Friend Shelf toggles, role editor storage grants, bucket browser read side, nav modes, `check-css-tokens` _Proven 2026-10-26 on the merged tree (integration-phase27.md): vitest on src/settings, src/storage, src/lounge (146 files, 1162 tests), typecheck, eslint, prettier, i18n-audit 0, check-css-tokens, build and bundle budget._
