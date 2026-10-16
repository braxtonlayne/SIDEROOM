# Handoff: feat/p14-fu-ops (phase 14, follow-up ops items)

Branch `feat/p14-fu-ops`, base `integration/phase14` (= `main`). Items: H-46, H-47, H-48 (answers-followups-p13), G-85, G-98 (answers-followups-p12). All five are ticked in the checklists.

## Done

- **H-46 animated WebP output.** `crates/sr-storage/src/img.rs`: animated GIF and animated WebP sources decode frame by frame (same frame and pixel budget as before; the WebP source through `image`'s WebP decoder), and encode as animated WebP with `webp::AnimEncoder` (lossy, quality 80, frame delays kept, loop forever) when the format is WebP, or as GIF. `pick_format`: an animation becomes WebP when the client accepts it, GIF otherwise (this changes the Accept-negotiated default for animated GIFs from "stays GIF"). The CDN reads the VP8X animation flag from the 32-byte head to treat animated WebP sources as animations. `?format=avif` is accepted by `variant.rs` and answers the WebP variant. `FND-STOR-003` ticked. Doc 05 image-variants rows updated.
  - Crate judgement: `webp` 0.3.1 (libwebp bindings) is listed in doc 03 section 4 ("Images" row) and named in ADR-0005, and `sr-library` already links it, so no ADR and no new C dependency; added to `sr-storage` with `default-features = false` (no second `image` coupling). Pure-Rust encoders have no animation or lossy mode.
  - Known limit: the crate's `AnimEncoder` terminates the stream with end timestamp 0, so the last frame's duration is whatever libwebp clamps it to rather than the source delay; earlier frames keep their delays.
- **H-47.** `REVIEW_CARD_IMAGE_TTL` (7 days) in `staff_feed.rs`, `signed_image(key, ttl)` over a new `MessagesDeps::sign_url_for(path, ttl)` (`sign_attachment_url` delegates, still 24 h). Every card write and rewrite goes through `review_info`, so each is re-signed. Test asserts `ex` ~ now + 7 days (`staff_feed_pg_tests.rs`).
- **H-48.** `deploy/compose/prod.yml` (read-only mount `${SR_INGEST_DIR:-/srv/sideroom/ingest}:/srv/sideroom/ingest:ro`, `SR_INGEST__ROOTS: ${SR_INGEST_ROOTS:-}`), `.env.prod.example` (both keys, default on), `deploy/install.sh` (prompt `SR_INGEST_DIR`, `none` turns it off, path validated, folder created when missing, warns if it cannot), `deploy/INSTALL.md`, doc 12 paragraph. `IngestConfig.roots` now also accepts a single comma-separated string (a compose environment line cannot carry a TOML array). Tests: `deploy_files.rs` (mount read-only, env keys; list-valued settings compare as comma-joined), `scripts/install.test.mjs`, `config.rs` unit test. **`deploy/install.sh` was 100644 in git; it is now 100755.**
- **G-85.** `crates/sr-db/migrations/0803_drop_plaintext_signing_seed.sql`: `DO` block raises "run the previous release once to seal seeds" while any `interactions_signing_seed IS NOT NULL`, then drops the column. Removed the dead startup job (`app.rs`) and `PgAppsRepo::legacy_seed_count`/`seal_legacy_seeds`. `secret_columns.rs`: the `PLAINTEXT_OK` entry is gone; the scanner now ignores a column that a later migration drops. Upgrade order and the rollback caveat (the one-version rule does not cover this migration) are in `deploy/UPGRADE.md`. Tests: `tests/migration_0803.rs` (guard precedes drop), `tests/apps_repo.rs` (pg, sealed-at-rest and rotation).
- **G-98.** Audit: every row was already written (G-90 to G-93 branches and the P13 mfa-protect branch). Ticked with the pointers; no doc change needed.

## Still open / owner

- The ingest mount on the production box is the owner's action (open-questions.md).
- Last-frame duration of animated WebP (above).
- Migration 0803 will stop an upgrade of a database that still has plaintext seeds; that is the specified behavior.

## Tests run
See the final report of the agent for the exact commands and results.
