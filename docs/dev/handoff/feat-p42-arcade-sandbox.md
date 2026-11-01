# feat/p42-arcade-sandbox (ARC-SBX-001, wave 42 worktree B) - handoff

## Resume here
Done and committed: web sandbox (opaque-origin iframe `allow-scripts allow-pointer-lock`, validated bridge, `storage` and `saveFile`
commands, SDK `sdk.storage`), proxy CSP (`sandbox` directive, `connect-src 'none'` unless the manifest `connect` list names hosts,
CORS `*` on bundle files), migrations 2050/2051, `sr_activities::{sandbox,trust}` + `service/trust_db.rs`, routes
`GET /activities/{id}/trust`, `PUT|DELETE /activities/{id}/vouch` (route_matrix.tsv rows + authz_matrix.rs skip entries),
`sr-cli arcade sign-code|revoke-code|backfill-hashes`, web `apps/web/src/activities/trust/` (interface for arcade-room), Land.io
storage wiring, Whiteboard save via bridge, i18n fragment p42-arcade-sandbox-1.ts.

Still to do, in this order:
1. e2e: update `e2e/tests/activities-live.spec.ts` lines ~144 and ~167 (CSP now starts `sandbox allow-scripts allow-pointer-lock; default-src 'none'`;
   `location.origin` in the frame is now `"null"`, check `location.hostname` instead). Write `e2e/tests/arcade-sandbox.spec.ts` (typecheck only, DO NOT RUN):
   Land.io via `startActivityForTwo`; from the frame `fetch('https://example.invalid')` and `fetch(location.href)` reject, `document.cookie`
   and `parent.document` throw; signed badge via `runCli(['arcade','sign-code','--hash',h])`; vouch flip after uploading changed code
   (vouching needs an account >= 3 days old: backdate `accounts.created_at` through psql like `e2e/support/storage-live.mjs` does).
   Then `pnpm --filter @sideroom/e2e typecheck` (find the exact script).
2. OpenAPI: `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, `pnpm --filter @sideroom/api-client generate`, mocks generate if present.
3. Threat-model row in docs/15-threat-model.md (opaque origin, CSP per host not per URL, audit is a tripwire not proof, signing key sealed in DB,
   vouch farming caps, featured badge only while code is signed). Also add the `sandbox`/`connect`/`storage` manifest docs to docs/11.
4. Gates: `cargo fmt --check`, `cargo clippy -p sr-activities -p sr-api -p sr-cli -D warnings -j 2`, sr-server `authz_matrix` (ignored, heavy) and
   `immutable_rules` if time; web `eslint`, bundle budget (`node scripts/check-bundle-budget.mjs` after build), full vitest --maxWorkers=2.
5. Final: fill "Tests run" and "Open owner questions" below.

## Tests run so far (all passed)
- vitest apps/web src/activities + src/i18n (103), embedded-app-sdk (27+), starter packs whiteboard/holdem/tabletop; landio 298 pass, 1 timing test
  (`draft vs classic ms`, load-sensitive) failed once - rerun alone.
- cargo: sr-activities lib (34), tests/trust_db (9, Postgres), tests/db (9), sr-db secret_columns, sr-api activities (13) and route_matrix (6),
  sr-cli arcade (3, Postgres). Scratch DB sr_p42_arcade_sandbox (drop it when finished).

## Decisions / findings
- Real bug fixed: approving an update failed on the unique "one approved bundle" index; `decide` now supersedes the old one first.
- `code_sha256` = SHA-256 over sorted .js/.mjs/.wasm only (domain-separated framing); art changes do not change it.
- Good standing = active, non-bot, account >= 3 days old; 30 vouches/day. OPEN OWNER QUESTION: confirm numbers.
- OPERATOR-FEATURED counts only while the current code is signed (an unsigned update loses it). OPEN OWNER QUESTION: confirm.
- `connect-src` 'self' is honoured only when the app has no URL mappings (a mapping is a pipe to the dev's server). URL-mapping writes from games are dead (Origin: null).
- Added `'wasm-unsafe-eval'` to script-src so signed .wasm can run; no `unsafe-eval`.
- Integration for the wave-42 integrator: `is_featured` probes `to_regclass('arcade_featured')` (arcade-room migration 2040); replace by a plain EXISTS.
  Conflict-prone shared files with arcade-room: bundle.rs, service.rs, model.rs, lib.rs. "vX" in the UNVERIFIED UPDATE text is the 8-char hash prefix (no version stored).
- Signing crate: ed25519-dalek 3.0.0 (already in Cargo.lock; no new crate).
