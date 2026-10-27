# Handoff: feat/p27-byos-instance (wave 27, BYOS-INS-001)

## Resume here
Slices 1-2 done and committed. Open, in order:
1. `sr-cli storage set-instance-mount` (the operator's way out of the bucket-change refusal; the refusal message and UPGRADE.md already name it). `crates/sr-cli` belongs to byos-core: ask the commander to add it, or allow byos-instance to (needs a `PgMountRepo` method to repoint mount 1: an `update` with `MountPatch{bucket,endpoint,region,credentials}` already exists).
2. `POST /admin/storage/mounts/1/attach|detach` (`crates/sr-api/src/routes/admin/storage.rs`, `OPERATOR`, audited): attaching mount 1 to a guild as a Guild Shelf needs `guild_storage_grants` from byos-scopes (migration 1410, not in this base). Do it after rebasing on byos-scopes; route_matrix rows + OpenAPI regen then.
3. Per-row mount threading (see Findings): only worth doing when a non-1 row can exist (wave 28). `MountStorages::for_mount(row.mount_id)` is ready.
Env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-byos-instance-target, CARGO_PROFILE_DEV_DEBUG=0, `-j 2`, one cargo at a time. Scratch DB `sr_p27_byos_instance` exists (drop at wave end). No server was started.

## Done
- Migration `1420_media_mount_id.sql`: `mount_id bigint not null default 1`, `source text not null default 'instance'` on `video_assets`, `video_titles`, `video_episodes`, `reading_series`, `reading_chapters` (the roadmap's `cinema_assets`/`cinema_titles`/`library_chapters`/`library_series` are these real tables). Untouched: attachments, emoji, avatars, stickers, profile media.
- `sr_storage::mount_storage` (new file, plus one `pub mod` line in lib.rs): `MountStorages::{instance_only,new,instance,for_mount}`; mount 1 always returns the config-built handle (never the DB, no prefix), other mounts resolve through `MountResolver`, are cached per `updated_at`, and wrapped in `PrefixedStorage` (mount prefix applied/stripped). 4 unit tests (`cargo test -p sr-storage --lib mount_storage`).

## Findings / risks
- Adding `mount_id` to `sr_media::assets::Asset`/`NewAsset` or the catalog row structs breaks struct literals in existing tests (`sr-api/src/routes/cinema_media/tests.rs:873`, `sr-db/tests/cinema_submissions.rs:130`, `sr-cli/src/tv.rs:155`), which conflicts with "existing tests pass unchanged". So row-carried `mount_id` is not threaded into those structs; all rows are mount 1 in wave 27 and resolve to the identical instance handle.
- The CDN path (`/cdn/{*path}`) names a key, not a mount; per-mount serving needs the URL to carry the mount, a wave-28/29 change. Not widened here.

## Done in slice 2
- `crates/sr-server/src/instance_mount.rs`: `ensure_instance_mount` seeds mount 1 (platform owner, provider garage, path style, sealed secret, key hint) from `storage.*`, idempotent, refuses when mount 1's bucket differs from `storage.bucket`. Called in `app.rs` right after the storage handle is built (skipped for `storage.endpoint = "memory:"`); `storage` is then `MountStorages::instance()`, the very same Arc, so every service is unchanged.
- `deploy/UPGRADE.md` note.

## Tests run
- `cargo test -p sr-storage --lib mount_storage` (4), `cargo test -p sr-server --lib instance_mount` with `--include-ignored` (2 unit + 1 Postgres: seed, sealed round trip, idempotence, refusal, migration 1420 defaults and columns on all five tables).
- Existing, unchanged: sr-db `media_repo` (5), `library_wiring` (16), `cinema_submissions` (8), `mounts_repo` (2) on the scratch DB with 1420 applied.
- clippy `-D warnings` and fmt clean on sr-storage and sr-server (all targets).
- Not run: the sr-api/sr-cinema/sr-library/sr-media suites (no code changed there), `media.yml`/`storage.yml` (CI), browser checks.
