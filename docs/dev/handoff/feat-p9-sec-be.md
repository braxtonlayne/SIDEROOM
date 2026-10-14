# Handoff: feat/p9-sec-be

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p9-sec-be, branch feat/p9-sec-be (base integration/phase9). Not pushed, not merged. No migrations.
Build env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p9-sec-be-target CARGO_PROFILE_DEV_DEBUG=0. cargo-nextest is not installed here, so `cargo test` was used.

## Done
- **XC-SEC-002** `sr-storage/src/upload/surface.rs`: `Surface` (ChatAttachment, ProfileImage, Expression) with a per-surface sniffed-MIME allowlist and `is_active_content` (SVG/HTML/XML detection). Wired into chat attachment finalize (markup is stored as `application/octet-stream`, never `text/plain` or an image) and `media::prepare_bytes` (markup refused before any decoder). Emoji/sticker upload keeps its own sniff (it also needs animation and Lottie detection) which already refuses everything but raster.
- **XC-SEC-003** new crate `sr-net`: `Policy`/`is_forbidden_ip` (moved from sr-unfurl), `Resolver`, `resolve_validated` (all answers checked, connect pinned), `pinned_builder`, `SafeClient` (capped GET, 3 revalidated redirects). sr-unfurl and the image proxy now use sr-net for policy, resolution and the pinned client builder (public API of sr-unfurl unchanged via re-exports). The payments esplora/rate-source client (`ReqwestGet`) now uses `SafeClient`; new opt-in `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1` for private-network nodes. Tests: private/loopback/link-local/metadata literals, names resolving private (also mixed answers), redirect to private, DNS flip between hops, redirect cap, size cap, schemes/credentials, operator opt-in.
- **XC-SEC-004** `SR_<KEY>_FILE` for all secret config keys (`sr_core::config::with_secret_files`, called by `Config::load`; both-set and empty/missing file are errors; a test fails if a new secret field is not listed), `SR_VOICE_TOKEN_SECRET_FILE` for the standalone SFU. Token audit: every stored credential is a digest or ciphertext; `sr-db/tests/secret_columns.rs` scans all migrations and fails on a plaintext credential column. Rotation docs: docs/12-deployment-and-operations.md section 3.1.
- **XC-SEC-005** `sr-api/tests/route_matrix.rs` + `route_matrix.tsv` (570 operations): fails when an OpenAPI operation has no row (or a row is stale), when a router path literal is neither in OpenAPI nor under `#undocumented`, when ACCESS disagrees with the handler's OpenAPI `security`, or when BOT/BEARER disagree with `auth::policy`. A second test sends an anonymous request to every authenticated route and expects 401. Regenerate rows with `SR_UPDATE_ROUTE_MATRIX=1`; new rows get a `?` that fails the test until reviewed.

## Tests run
cargo fmt; clippy -D warnings (sr-net, sr-unfurl, sr-storage, sr-core, sr-payments, sr-api, sr-voice, sr-server, sr-db, all targets); cargo test for sr-net (14), sr-unfurl, sr-storage lib (50), sr-core config (13), sr-db secret_columns, sr-api route_matrix (3). No Postgres-backed code changed, so no ignored DB tests were run and no scratch DB was created.

## Decisions
- SVG: reject, do not sanitize (no sanitizer in the workspace; a new parser is more attack surface than the feature is worth). Attachments of SVG/HTML stay downloadable but are never given an inline type.
- Invite, template and watch-party codes stay plaintext (issuer re-displays them; narrow capability). Listed in the audit test.
- Discord OAuth talks only to fixed discord.com URLs and keeps its own no-redirect client; not moved to sr-net.
- Payments client blocks private hosts by default (behaviour change for operators with a LAN esplora node; see env flag).

## Open owner questions / limits
- The AUTHZ column of the matrix is heuristic-seeded from the path (staff/owner/membership/self/other), not hand-verified per handler; 160 rows are `other`. It pins access, bot and bearer policy and the anonymous 401, not each permission bit.
- Nine `/sr/auth/*` routes (listed in `NEEDS_DEPS_FIRST`) extract `AuthService` before `Principal`, so with no service they answer 500 rather than 401. Suggest reordering extractors.
- The router-literal scan is textual (`.route("...")`); routes registered otherwise (cdn, gateway, feature nests) rely on being under `#undocumented`.
- Outbound webhooks and RSS/metadata fetchers do not exist yet; they must use `sr_net::SafeClient`.
- Video and library ingest keep their own decoders and were not changed.
