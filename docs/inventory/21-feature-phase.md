# 21 — Feature phase (`BYOS`, and the later prefixes of roadmap-p27)

Waves 27 to 40. Plan, acceptance criteria, agent split and owner needs: [dev/roadmap-p27.md](../dev/roadmap-p27.md) (D-380). Prefixes are those of roadmap-p27 section 4; each wave's integrator adds its items here.

Same conventions as [20-launch-readiness.md](20-launch-readiness.md): an item ticks only when its proof runs (a CI job, a Postgres or browser test, or a recorded run in the integration handoff), never when it is only written. Items whose live proof needs CI (the Garage cases of `storage.yml`) say so and stay unticked until that run is linked.

## Wave 27 — BYOS core

Integration: [dev/handoff/integration-phase27.md](../dev/handoff/integration-phase27.md).

### BYOS-MNT — Mounts
- [ ] **BYOS-MNT-001** [DB][BE] Mounts and sealed credentials: migration 1400 `storage_mounts`, `sr_storage::mounts` (`MountRepo`, `MountResolver`, sealed credentials), `CachedMountResolver`, user and guild mount CRUD routes, `sr-cli` rotate for mounts, `sr_storage::path::safe_key`
- [ ] **BYOS-MNT-002** [BE] Connection test and provider guidance: `POST .../mounts/{mount_id}/test` (pass/warn/fail rows, 20 s budget), `abuse.storage_tests_per_hour`
- [ ] **BYOS-MNT-003** [BE][CLI] CORS checker and apply: `.../cors/check`, `.../cors/apply`, daily job `storage.cors_recheck` (id 134), `sr-cli storage ensure-cors`

### BYOS-SCP — Scopes
- [ ] **BYOS-SCP-001** [DB][BE] Scopes, shares and grants: migration 1410, `sr_storage::scopes::resolve`, Friend Shelf shares, guild storage grants, unfriend hook
- [ ] **BYOS-SCP-002** [BE] Scoped listing and presign: objects listing, metadata, presign (TTL settings `byos.presign_*`), authz matrix storage actors

### BYOS-INS — The instance bucket
- [ ] **BYOS-INS-001** [DB][BE] The instance bucket as mount 1: migration 1420 (`mount_id`, `source`), `MountStorages`, mount 1 seeded at start, a changed bucket refused

### BYOS-UI — Settings
- [ ] **BYOS-UI-001** [FE] User Settings > Storage and Guild Settings > Storage, Friend Shelf toggles, role editor storage grants, bucket browser read side, nav modes, `check-css-tokens`
