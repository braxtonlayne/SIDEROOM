# feat/p28-byos-legal handoff (BYOS-LEG-001, OPS-IDX-001)

## Resume here
Both items are built; what is left is verification and the follow-ups below. Order:
1. Run the Rust gates not yet run (see "Not yet run"): `cargo test -p sr-api --test api security_headers_on_every_response`, `--test route_matrix`, `--test openapi_names`, the authz matrix (sr-server, Postgres), clippy `-D warnings` on sr-db, sr-moderation, sr-storage, sr-admin, sr-api, sr-voice, sr-server tests.
2. Regenerate OpenAPI: `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then `pnpm --filter @sideroom/api-client generate` (and `mocks:generate` if the script exists). The new schemas are `AdminStorageMount`, `StorageMountLegal`, the `NewItem.key` field, `TakedownItem.object_key`, item kind `storage_object`.
3. Web gates: `tsc`, eslint, prettier, vitest (`src/admin/__tests__/p28storage.test.tsx`, `src/i18n/catalog.test.ts`, the logical-CSS test), build + `scripts/check-bundle-budget.mjs`.
Worktree has node_modules (offline install done); wasm is not built (vitest ran without it).
Scratch DB `sr_p28_byos_legal` exists on the local Postgres: drop it when done (`DROP DATABASE sr_p28_byos_legal`).

## Done
### OPS-IDX-001 (D-384): only the landing page is indexable
- `apps/web/public/robots.txt` (`Allow: /$`, `Disallow: /`, no sitemap).
- `deploy/compose/caddy/Caddyfile.prod`: `@notLanding not path /` and `header @notLanding X-Robots-Tag "noindex, nofollow"` at site level (covers /api, /gateway, /cdn, /healthz, /developers, /internal and the SPA except `/`). Set per matcher rather than removed in the SPA handle so it does not depend on directive order. `activities.snippet` sets the header on `*.activities.<domain>`. `render-caddyfile.mjs` only rewrites the CSP line, so it needed no change; `apps/web/src/security/csp.test.ts` pins the lines and the robots.txt bytes.
- `crates/sr-api/src/security_headers.rs` (every sr-server response), `routes/activities/proxy.rs::harden`, `crates/sr-voice/src/ws.rs::noindex` (layer in `router()` and again in `main.rs` for `/metrics`).
- SPA: `apps/web/src/lib/useNoindex.ts` called from `RootLayout` in `router.tsx` (meta added on every pathname but `/`, removed on unmount).
- `e2e/tests/noindex.spec.ts`, run by the prod-smoke `stack` job (`entrance-smoke noindex`); YAML validated with js-yaml. Not run locally (needs the Caddy stack).
- docs: 09 sentence, 15 row T28.

### BYOS-LEG-001: takedown and delisting for user-owned buckets
- Migration `1460_storage_object_takedown.sql`: `takedown_items.kind` gains `storage_object`, `object_key` column (target_id = mount id), unique index per (notice, kind, target, key), `storage_mounts.status` gains `delisted` plus `delisted_at/by`, `delist_reason`, `delist_prior`.
- `sr-moderation`: `ItemKind::StorageObject`, `NewItem.key`, `TakedownItem.object_key`; item key validation; no hold and no `blocked_objects` row for a storage object (its key means nothing in the instance's namespace); uphold, uploader DM (D-336 (c)), counter-notice, restore, strikes all reuse the existing generic paths. New `takedown_mounts.rs`: `delist_mount`, `relist_mount` (reason required, access log rows `storage.mount.delist|relist`, owner DM), `list_mounts`.
- `sr-db::storage_legal`: `delisted_keys` (now behind `ScopeFacts::delisted`, replacing the empty stub in `scopes_repo.rs`), `delist_mount`, `relist_mount`, `admin_list`, `export_for`, `erase_for`. `mounts_repo.rs` SQL no longer lets a connection test or an owner patch move a mount out of `delisted`.
- `sr-storage`: `MountStatus::Delisted`; `scopes::access` refuses a delisted mount for everyone, owner and staff included (unit test added).
- Ban or suspension: no row is needed, `ScopeFacts::owner_active` already refuses every mount of an inactive account and restores them when the ban lifts.
- DSR: export gains `storage.json` (mounts: label, provider, bucket, prefix, status, delisted-object count, shares; shelves shared with the member; never keys, key ids, hints or object keys); erasure (`account_deletion::delete_account`) soft-deletes the mounts, wipes `secret_key_enc` and the key id, removes shares; the request outcome text mentions it (`ErasureReport.mounts_erased`).
- API: `GET /admin/storage/mounts`, `POST /admin/storage/mounts/{mount_id}/delist|relist` (`routes/admin/storage.rs`, MANAGE_TAKEDOWNS), route matrix rows added by hand (staff, MANAGE_TAKEDOWNS), authz_matrix seed block `legal_mount_id` plus an OVERLAYS entry in `crates/sr-server/tests/authz_matrix.rs`.
- Web: `apps/web/src/admin/storage/*` (section `storage`, People group), takedown tab accepts kind "Object in a member bucket" (mount id + key), catalog fragment `p28-byos-legal-1.ts`, test `p28storage.test.tsx`.
- e2e: `moderation-gate.spec.ts` gains the storage step (create a bucket, notice against one object, uphold, delist and relist the bucket, counter-notice, restore) and its access-log counts (takedown rows now 2, plus storage.mount.delist/relist). Not run (ports 8080/5173 are reserved this wave).
- docs: 09 matrix rows (mounts, objects, delist records, presigns) and DSR paragraph; 15 rows T29 and T30; `docs/legal/lawyer-brief.md` Part E with Q20, Q21, Q22, Q23 (the roadmap assigned Q21 to wave 35 and Q22/Q23 to wave 40; the task for this wave asked for all four, so later waves must not add them again).

## Tests run
- vitest: `useNoindex.test.tsx` (11), `csp.test.ts` (13).
- Postgres (scratch DB): `cargo test -p sr-moderation --test storage_takedown -- --include-ignored --test-threads=1` 4 passed (delist, counter-notice restore, input checks, whole-mount delist/relist, export without keys, erasure wipes `secret_key_enc`).
- `cargo test -p sr-voice --lib noindex` passed. `cargo check` for sr-moderation, sr-storage, sr-db, sr-admin, sr-api with tests: clean.

## Not yet run
- `crates/sr-db/tests/storage_legal.rs` (Postgres, new), the `scopes` unit test for `Delisted`, sr-api `security_headers_on_every_response` (extended), `route_matrix`, `openapi_names`, authz_matrix, clippy, fmt check on all touched crates, web gates, the e2e specs (CI only).

## Follow-ups and open items
- Owner screen: the member's own Settings > Storage does not yet show a "Delisted" badge with the reason (the owner gets the DM). `settings/user/storage/*` belongs to byos-browser.
- Catalog/index rows: byos-catalog owns them. Erasure and delisting should hide rows with `source = 'mount'` and the `mount_id` of an erased or delisted mount; the register and `scopes::resolve` are ready, the catalog must call `delisted_keys` / check the mount status when it lists a shelf. Integrator to confirm.
- Guild-owned mounts: a notice names the guild owner as the subject (strike target); a guild mount is not erased with a member account (the guild must be transferred first).
- `MountObject.status` doc comment in `routes/storage/mounts.rs` still lists three statuses; `delisted` can now appear there.
- Connect wizard guidance ("keep the bucket's listing off", roadmap OPS-IDX-001 (d)) not added (byos-browser's wizard).

## Owner questions
None blocking. For the lawyer: Q20 to Q23 are in Part E of the brief.
