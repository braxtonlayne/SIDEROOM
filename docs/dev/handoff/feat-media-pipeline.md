# Handoff: feat/media-pipeline

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/media, branch feat/media-pipeline (base integration/phase1). Not pushed, not merged.
Build env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/media-target CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0.
Scratch database: `sr_media` (left in place, migrated by the ignored tests). The shared `sr_test` has migration 130 from another branch, so sr-db tests fail there with VersionMissing(130); use sr_media.

## Done and verified
- sr-storage image pipeline (img.rs, media.rs, cdn.rs variant signatures): unit tests.
- /cdn variants (?size=, format=, Accept, cache `<key>@<size>.<ext>`, 413/415, public avatars/icons): router tests.
- User avatar/banner on PATCH /users/@me; GET /users/@me avatar+banner; guild icon/banner/splash on PATCH /guilds/{id}; emoji/sticker signed URL endpoints; MediaDeps wired in sr-server app.rs.
- Gate: cargo fmt --check, clippy -D warnings, cargo test --workspace (exit 0), cargo deny check, ignored Postgres tests on sr_media (exit 0), OpenAPI + api-client regenerated, cdnUrl vitest (3 pass).

## Not done / unverified
- No S3 server locally: nothing ran against real S3. e2e/support/storage-live.mjs section 9 (avatar, icon, emoji URL checks) only passed `node --check`; CI's Garage workflow is the verification.
- AVIF output not implemented (FND-STOR-003 left unticked). WebP output is lossless only; animated WebP -> first frame.
- Full web test suite not run (only cdn.test.ts).

## Decisions for the owner
- /cdn/avatars and /cdn/icons are unsigned (content-hash credential); a present-but-invalid signature is 403. Documented in docs/05 section 5.2 as an exception to docs/02 line 140.
- PATCH /users/@me `banner` is now an image data URI (was an opaque key).
