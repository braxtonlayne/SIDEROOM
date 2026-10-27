# Handoff: feat/p28-byos-catalog (BYOS-CAT-001, BYOS-CAT-002)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-byos-catalog`, base `integration/phase28`.
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-byos-catalog-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`.
Scratch DB `sr_p28_byos_catalog` (not created yet). Migration 1450 only. Job id 1450 / kind `storage.index`.

## Resume here

Slice 1 done: migration `1450_storage_objects.sql` (not yet run against Postgres), `crates/sr-storage/src/index/`
(`classify.rs`, `sniff.rs`, `mod.rs` with the `IndexStore` trait), 15 unit tests green.

Next: (2) `sr-db` index store (`index_repo.rs`: `storage_objects` upsert, catalog rows for Cinema/Library/Music,
re-index keeping `overridden` fields, vanished handling) with Postgres tests; (3) walker + job `storage.index` in
`crates/sr-server/src/storage_jobs.rs`, the `sr_media::naming` adapter, the 200-name test; (4) routes (re-index,
shelves read, editor PATCH) + route_matrix + authz seeds; (5) `mount_id` threading and per-mount media URLs;
(6) web shelves and editor; (7) docs (load-testing row, naming spec section).

## Design decisions

- Catalog rows from a mount are written with `published = FALSE`, so every existing member query (published and
  not merged) skips them and nothing leaks by omission. The shelf queries read them by `source = 'mount'`,
  the mounts `scopes::access` admits for the viewer's kind, and `shelf_state = 'listed'`.
- An edit is an override: the catalog row's `overridden TEXT[]` names the fields a member edited; re-index never
  rewrites them. Series rows are anchored by `(mount_id, shelf_key)` (folder), episodes and chapters by
  `mount_object_id`, so a rename in the editor does not create a duplicate.
- sr-storage cannot depend on sr-media (cycle), so video names reach the classifier through the `NameParser` trait;
  the adapter over `sr_media::naming::parse_path` lives in sr-server.
- Header read: `index::sniff` (container signature, MP4 codec tags) is the only place that reads file bytes.

## Open / not done
(updated each slice)
