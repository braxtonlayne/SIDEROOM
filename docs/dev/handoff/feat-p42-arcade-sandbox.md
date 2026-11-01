# feat/p42-arcade-sandbox (ARC-SBX-001, wave 42 worktree B) - handoff

## Resume here
Everything on the list is done and committed. Nothing is left except what the wave-42 integrator reconciles (below) and running the two
Playwright specs on a free 5173/8080: `e2e/tests/arcade-sandbox.spec.ts` (new, typechecked, never run) and the amended
`e2e/tests/activities-live.spec.ts` (sandbox CSP, opaque origin `"null"`). Drop the scratch DB `sr_p42_arcade_sandbox` when done.

Built: opaque-origin sandbox + validated bridge (`storage`, `saveFile`), sandbox CSP/CORS in the proxy, migrations 2050/2051,
`sr_activities::{sandbox,trust}`, routes `GET /activities/{id}/trust`, `PUT|DELETE /activities/{id}/vouch` (both matrices; the authz seed now
releases the matrix application with an approved build), `sr-cli arcade sign-code|revoke-code|backfill-hashes`, web interface
`apps/web/src/activities/trust/` (`useBuildTrust`, `BuildTrust`, `BuildBadges`), Land.io storage wiring, Whiteboard save via the bridge,
threat model row T39 (arcade-room may also take T39: renumber on merge), OpenAPI/api-client/mocks regenerated.

## Tests run (all passed)
- Gates: cargo fmt; clippy -D warnings (sr-activities, sr-api, sr-cli, all targets); web tsc, eslint, prettier, full vitest (424 files), build + bundle budget;
  sr-server authz_matrix (8) and immutable_rules (22); e2e tsc.
### Earlier runs
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
