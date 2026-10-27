# feat/p28-byos-legal handoff (BYOS-LEG-001, OPS-IDX-001)

## Resume here
Both items are built and their gates ran (see "Tests run"). Left for the integrator or a follow-up agent:
1. Web production build and `scripts/check-bundle-budget.mjs` (needs `pnpm --filter @sideroom/wasm build`, a cargo build, so not run here). The new web code is one small admin section plus a hook.
2. The two Playwright specs (`e2e/tests/noindex.spec.ts` via prod-smoke on CI, `moderation-gate.spec.ts` storage step) have not run: they need the Caddy stack / ports 8080 and 5173.
3. The follow-ups listed below.
The scratch database `sr_p28_byos_legal` was dropped; recreate it to rerun the Postgres tests.

## Done
### OPS-IDX-001 (D-384): only the landing page is indexable
- `apps/web/public/robots.txt` (`Allow: /$`, `Disallow: /`, no sitemap).
- `deploy/compose/caddy/Caddyfile.prod`: `@notLanding not path /` and `header @notLanding X-Robots-Tag "noindex, nofollow"` at site level (covers /api, /gateway, /cdn, /healthz, /developers, /internal and the SPA except `/`). Set per matcher rather than removed in the SPA handle so it does not depend on directive order. `activities.snippet` sets the header on `*.activities.<domain>`. `render-caddyfile.mjs` only rewrites the CSP line, so it needed no change; `apps/web/src/security/csp.test.ts` pins the lines and the robots.txt bytes.
- `crates/sr-api/src/security_headers.rs` (every sr-server response), `routes/activities/proxy.rs::harden`, `crates/sr-voice/src/ws.rs::noindex` (layer in `router()` and again in `main.rs` for `/metrics`).
- SPA: `apps/web/src/lib/useNoindex.ts` called from `RootLayout` in `router.tsx` (meta added on every pathname but `/`, removed on unmount).
- `e2e/tests/noindex.spec.ts`, run by the prod-smoke `stack` job (`entrance-smoke noindex`); YAML validated with js-yaml. Not run locally (needs the Caddy stack).
- docs: 09 sentence, 15 row T29 (T28 at the branch; renumbered at integration).

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
- docs: 09 matrix rows (mounts, objects, delist records, presigns) and DSR paragraph; 15 rows T30 and T31 (T29 and T30 at the branch; renumbered at integration); `docs/legal/lawyer-brief.md` Part E with Q20, Q21, Q22, Q23 (the roadmap assigned Q21 to wave 35 and Q22/Q23 to wave 40; the task for this wave asked for all four, so later waves must not add them again).

## Tests run
- vitest (`--maxWorkers=2`): `src/lib/useNoindex.test.tsx`, `src/security/csp.test.ts`, and all of `src/admin`, `src/moderation`, `src/security`, `src/lib` (63 files, 484 tests, incl. the new `p28storage.test.tsx` and the updated nav list in `admin.test.tsx`); `src/i18n` (catalog tests). tsc, eslint and prettier clean on apps/web, e2e tsc clean.
- Postgres (scratch DB, `--test-threads=1`): `sr-moderation --test storage_takedown` 4 passed; `sr-moderation --test takedowns` 17 passed (existing, after the NewItem change); `sr-db --test storage_legal` 1 passed; `sr-db --test mounts_repo` 3 and `--test scopes_repo` 3 passed; `sr-server --test authz_matrix` 4 passed (first run failed only because another agent held UDP 52000-52019; rerun green).
- Rust without a database: `sr-storage --lib` 111, `sr-admin` 62, `sr-voice --lib` 38, `sr-api --test route_matrix` 6, `--test api` (security_headers_on_every_response, every_response_says_noindex, hsts) 3, `--test openapi_names` 2.
- `cargo fmt --check` and `cargo clippy --all-targets -D warnings` clean on sr-db, sr-moderation, sr-storage, sr-admin, sr-api, sr-voice (fmt also sr-server). OpenAPI, `schema.d.ts` and msw handlers regenerated and committed.

## Follow-ups and open items
- Owner screen: the member's own Settings > Storage does not yet show a "Delisted" badge with the reason (the owner gets the DM). `settings/user/storage/*` belongs to byos-browser.
- Catalog/index rows: byos-catalog owns them. Erasure and delisting should hide rows with `source = 'mount'` and the `mount_id` of an erased or delisted mount; the register and `scopes::resolve` are ready, the catalog must call `delisted_keys` / check the mount status when it lists a shelf. Integrator to confirm.
- Guild-owned mounts (reworded at integration by P28-LEG-1): a notice names as the subject (strike target) the member who uploaded the object when the upload ledger knows them (`storage_mount_uploads`, latest `completed` row), else the guild owner; the guild owner is also told when not the subject (same text, no strike). A guild mount is not erased with a member account (the guild must be transferred first).
- `MountObject.status` doc comment in `routes/storage/mounts.rs` still lists three statuses; `delisted` can now appear there.
- Connect wizard guidance ("keep the bucket's listing off", roadmap OPS-IDX-001 (d)) not added (byos-browser's wizard).

## Owner questions
None blocking. For the lawyer: Q20 to Q23 are in Part E of the brief.
