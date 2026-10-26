# Handoff: feat/p27-byos-instance (wave 27, BYOS-INS-001)

## Resume here
Slice 1 done (migration 1420, `sr_storage::mount_storage::MountStorages`). Next, in order:
1. Seed mount 1 in `crates/sr-server/src/app.rs` after the storage handle is built (skip when `storage.endpoint == "memory:"`): `PgMountRepo` + `SecretBox` (sr_db::sealed, same master key as `apps_repo`), create mount 1 (`MountOwner::Platform`, provider garage/path-style, config endpoint/region/bucket, access key, sealed secret) if absent; refuse to start when mount 1 exists with a different bucket (message names `sr-cli storage set-instance-mount`; the subcommand lives in sr-cli, byos-core owns that file: ask the commander). Idempotent. Postgres test for seed + refusal.
2. Build `MountStorages::new(storage, pg_repo, secret_box)` in app.rs and hand `mounts.instance()` (identical Arc) to the services; thread `MountStorages` + row `mount_id` only where a row struct can carry it without editing existing test literals (see Findings).
3. `POST /admin/storage/mounts/1/attach|detach` (`crates/sr-api/src/routes/admin/storage.rs`): attaching needs `guild_storage_grants` from byos-scopes (migration 1410, not in this base); implement after rebase on byos-scopes.
4. `deploy/UPGRADE.md` note (no operator action).
Env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-byos-instance-target, CARGO_PROFILE_DEV_DEBUG=0, `-j 2`, one cargo at a time. Scratch DB sr_p27_byos_instance (not yet created).

## Done
- Migration `1420_media_mount_id.sql`: `mount_id bigint not null default 1`, `source text not null default 'instance'` on `video_assets`, `video_titles`, `video_episodes`, `reading_series`, `reading_chapters` (the roadmap's `cinema_assets`/`cinema_titles`/`library_chapters`/`library_series` are these real tables). Untouched: attachments, emoji, avatars, stickers, profile media.
- `sr_storage::mount_storage` (new file, plus one `pub mod` line in lib.rs): `MountStorages::{instance_only,new,instance,for_mount}`; mount 1 always returns the config-built handle (never the DB, no prefix), other mounts resolve through `MountResolver`, are cached per `updated_at`, and wrapped in `PrefixedStorage` (mount prefix applied/stripped). 4 unit tests (`cargo test -p sr-storage --lib mount_storage`).

## Findings / risks
- Adding `mount_id` to `sr_media::assets::Asset`/`NewAsset` or the catalog row structs breaks struct literals in existing tests (`sr-api/src/routes/cinema_media/tests.rs:873`, `sr-db/tests/cinema_submissions.rs:130`, `sr-cli/src/tv.rs:155`), which conflicts with "existing tests pass unchanged". So row-carried `mount_id` is not threaded into those structs; all rows are mount 1 in wave 27 and resolve to the identical instance handle.
- The CDN path (`/cdn/{*path}`) names a key, not a mount; per-mount serving needs the URL to carry the mount, a wave-28/29 change. Not widened here.
