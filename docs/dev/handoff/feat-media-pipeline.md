# Handoff: feat/media-pipeline

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/media (branch feat/media-pipeline, base integration/phase1).
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/media-target. No push, no merge.
Scratch DB `sr_media` was never created (nothing live was run). No processes running.

## Goal
FND-STOR-003 image proxy/resizer; user avatar/banner upload; guild icon/banner/splash; emoji/sticker signed URL endpoint; gateway shapes; web cdnUrl helper; CI live-S3 checks.

## DONE (compiles; unit-tested where noted)
- sr-storage: `img.rs` pure-Rust pipeline (probe/decode/transform/encode, budgets, animated GIF kept, WebP lossless, EXIF-free by re-encode) with 12 unit tests passing. `media.rs` (data URI parse, normalize, salted hash, keys `avatars/<user>/<hash>`, `icons/<guild>/<hash>`) tests passing. `cdn.rs`: `variant_canonical`, `signed_variant_url`, `verify_query` covers size/format; bare-path URLs still valid; test passing.
- sr-api `/cdn`: `variant.rs` (size 16..4096 power of two, format=/Accept/URL ext, cached at `<key>@<size>.<ext>`, 413 over budget, 415 non-image, 304, HEAD), `path.rs` public kinds avatars/icons (extension = format, unsigned allowed, immutable cache), semaphore + limits in CdnDeps. 14 new tests in `variant_tests.rs` passing (37 cdn tests green).
- Committed: sr-storage, cdn variants.

## IN PROGRESS (uncommitted before this wip commit; compiles with `cargo check --workspace --all-targets`)
- User avatar/banner: `UserSummary.banner_key`, `ProfileEdit.avatar_key`, `put_profile` persists avatar_key, `routes/media.rs` (MediaDeps store/remove, body_limit), `edit_self` handles `avatar`/`banner` data URIs with cleanup, `UserObject.banner`, route body limit 12 MiB.
- New tests in `routes/social/tests.rs` (`avatar_upload_replace_and_remove`, `bad_avatar_uploads_...`, updated `profile_edit_and_view`) and `routes/media.rs` tests: COMPILE BUT HAVE NOT BEEN RUN. Run: `cargo test -p sr-api --lib social::` and `media::`.

## REMAINING
- [ ] Register `MediaDeps::new(storage.clone())` in crates/sr-server/src/app.rs (not done; PATCH with images 500s without it).
- [ ] GET /users/@me (`openapi.rs CurrentUser.avatar` still null; fill avatar/banner from account) and READY user object.
- [ ] Guild icon/banner/splash: add banner_key/splash_key to `sr_chat::domain::Guild` (+ guild_rules, perms test, chat_repo GUILD_COLS/guild_from, guild_admin_repo update_guild), `GuildPatch` fields, `ModifyGuildBody` icon/banner/splash handling in guilds/guild.rs using MediaKind::Guild*, `GuildObject.icon/banner/splash`, GUILD_UPDATE already uses GuildObject; gateway_pg GUILD_CREATE (line ~183 has icon). Columns already exist (0010_guilds.sql) so no migration needed (migrations 0100-0109 unused).
- [ ] Emoji/sticker `GET /guilds/{id}/emojis/{eid}/url?size=` (and stickers) returning signed variant URL via `CdnSigner::signed_variant_url` (routes/settings/expressions.rs; Emoji.media_key is the object key).
- [ ] Postgres #[ignore] tests for guild columns; run ignored PG tests (SR_TEST_DATABASE_URL, --test-threads=1).
- [ ] Web helper `cdnUrl(kind,id,hash,size)` in apps/web/src/store (do not touch src/lounge).
- [ ] e2e/support/storage-live.mjs: avatar upload + resized CDN fetch checks.
- [ ] Docs: doc 05 section 5.2 (variants, public kinds, `?size=` note), tick FND-STOR-003 and avatar items; regenerate OpenAPI (bash: `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then `npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/api-client generate`).
- [ ] fmt, clippy -D warnings, full cargo test --workspace.
- [ ] Live verify: server needs real S3 (config only supports S3); none locally, so live S3 stays unverified (CI Garage covers).

## Decisions for owner
- avatars/icons URLs are served WITHOUT signature (128-bit content hash is the credential, like Discord); conflicts with docs/02 line 140 "buckets never public / URLs signed". Signed URLs still accepted. Confirm or require signing.
- WebP output is lossless only (image crate); animated WebP -> first frame; animation only as GIF.
- `banner` on PATCH /users/@me is now a data URI (was an opaque key); existing test updated.
- No fast_image_resize added; uses image::imageops (Lanczos3 stills, Triangle animated).
