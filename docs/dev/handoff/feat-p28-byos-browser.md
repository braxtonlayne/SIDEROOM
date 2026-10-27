# Handoff: feat/p28-byos-browser (BYOS-BRW-001 and three wave-27 follow-ups)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-byos-browser`, base `integration/phase28`. Env: `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-byos-browser-target`, `-j 2`, one cargo at a time,
`CMAKE` = the VS 2019 Build Tools copy (see integration-phase27.md) for anything that links sr-server.
Scratch DB `sr_p28_byos_browser` exists (drop it at the end of the wave). No python on this machine: edit with node or
the Edit tool; the Bash tool chokes on big heredocs that contain quotes, so write files with the Write tool.
Nothing pushed, no PR, no server was started.

## Resume here

Backend and web write side are done and committed. Left: (1) regenerate OpenAPI / api-client / MSW once (`cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json` from bash, then `pnpm --filter @sideroom/api-client generate` and `mocks:generate`): the integrator may redo it; (2) the live `storage.yml` Garage case (CI only); (3) doc 05 section; (4) the Delisted reason text once byos-legal's API carries one.

## Done (all with tests that ran)

- **Follow-up 3, settings.** `byos.max_mounts_per_user` (3), `byos.max_mounts_per_guild` (5) and `byos.upload_parts_per_hour`
  (2000) in the sr-admin catalog (group `storage`, one block marked "wave 28 byos-browser"). `mounts::cap_for` reads them
  live; the `DEFAULT_MAX_MOUNTS_PER_*` constants stay the fallback when no admin service is wired.
  MERGE NOTE: BYOS-FLG-001 (imm-flags) lists the same two mount-cap keys in its `byos.*` block: keep one copy.
- **Uploads through the ledger.** Migration 1440 (`storage_mount_uploads`, `storage_mount_part_notes`); `sr_storage::uploads`
  (`UploadLedger`, `MountIo`, `S3MountIo`, `MemoryLedger`, `MemoryMountIo`, `plan_move`); `sr_db::mount_uploads_repo::PgUploadLedger`;
  `S3Multipart` gained folder markers (`put_marker`), `delete_object`, `copy_object` (CopyObject, 5 GiB), raw `list_keys`
  (markers included), virtual-hosted addressing and a timeout.
- **Routes** (`crates/sr-api/src/routes/storage/uploads.rs`, `objects.rs` write side, AUTHZ `storage_scope`, session only):
  see the table below. Who may: `scopes::may_upload` for uploads and folders, `may_manage` for move and delete; staff only look.
- **Sweep job** `storage.uploads_sweep` (id 138, hourly, `crates/sr-server/src/upload_jobs.rs`): aborts open uploads idle for
  24 h at the bucket, closes them, drops part notes older than an hour; a dead bucket is retried for 7 days, then closed.
  Id 138 and the kind were checked for duplicates; byos-catalog must not take 138.
- **Follow-up 2, DNS rebinding.** `sr_storage::guard::EndpointGuard` (implemented by `routes::storage::NetGuard` over the
  SSRF policy): the signed clients (`S3Admin`: test, CORS check/apply; `S3Multipart`: every write route; the daily CORS recheck
  job) resolve the endpoint once, check every answer, and connect only to those addresses (`resolve_to_addrs`, tested with an
  unresolvable name). **Not pinnable:** `object_store`'s own client (listing, metadata, catalog header reads): it is reqwest 0.12
  inside object_store with no resolver hook, and a custom `HttpService` over our reqwest 0.13 needs the `http`/`http-body`
  crates as direct dependencies (not allowed this wave). Those paths re-check the endpoint through the guard right before the
  call (`objects::vet_endpoint`, list and meta), which narrows the window to milliseconds; the platform mount is not vetted.
  Follow-up: an `HttpService` over the pinned reqwest, when dependencies may be added. byos-legal's threat-model row should
  say "pinned except object_store reads, re-checked".
- **Follow-up 1, attach/detach.** Migration 1441 (`storage_mount_attachments`); `ScopeStore::{guild_exists, attach_mount,
  detach_mount}`; `PgScopeFacts::attached_guilds` now reads the table; routes `POST /admin/storage/mounts/{platform_mount_id}/attach`
  and `/detach` (body `{guild_id}`) and `GET .../attachments`, `OPERATOR`, audited (`storage.mount_attach|detach`), in
  `routes/storage/attach.rs` (not `routes/admin/storage.rs`, which is byos-legal's). The path id is `platform_mount_id` so
  the authz matrix does not treat it as a tenant id. The shelf is read-only for members (`may_upload` is false for the
  platform mount). No Admin UI button yet (byos-legal owns `apps/web/src/admin/storage/*`).
- **Rate limits.** `POST /storage/mounts/{mount_id}/uploads` rides `limits.rate.uploads` (route key added next to the attachment
  one in `rate_limit.rs`); part URLs ride `byos.upload_parts_per_hour`; at most 50 open uploads per member.
- **Route matrix** rows for the nine write routes and the three attach routes (`storage_scope`; `staff` + `OPERATOR`).
- **Authz matrix** (`crates/sr-server/tests/authz_matrix.rs`; only my own blocks, marked `BYOS-BRW-001`): a seeded open upload per
  tenant (`mount_upload_id`, added to `TENANT`, whose length goes 33 to 34), a seeded platform mount (`platform_mount_id`), the
  `part_number` fill value, bodies for the POST routes, and the new test `storage_writes_hold_over_http` (a friend with a share
  cannot upload, 404 for strangers and other tenants, the server owner is admitted, no CORS = 409 `CORS_NOT_READY`,
  disabled = 404). Group 5 (rest) and group 1 (admin) ran: every new route is admitted for the rightful actor and none adds a
  weak refusal or a malformed body. Group 5 still fails with exactly the seven failures of the guild-mount seeding that the
  wave-27 integrator is fixing (five `/guilds/{guild_id}/storage/mounts/...` writes, and the weak-refusal and malformed ceilings
  at 17 and 47); none of my routes is in them.

### Route shapes

All under `/api/v1`, session cookie only (bots and bearer tokens get 403). 409 errors use SrConflict 900010 with the string
code at `errors.mount._errors[0].code`.

| Route | Body / answer |
|---|---|
| `POST /storage/mounts/{mount_id}/uploads` | `{key, size, content_type?}` -> 201 `{id, key, size, part_size (16 MiB), part_count, content_type, created_at}`; 409 `CORS_NOT_READY`, `TOO_MANY_UPLOADS`, `BUCKET_UNREACHABLE` |
| `GET /storage/mounts/{mount_id}/uploads` | the caller's open uploads (newest first) |
| `GET .../uploads/{mount_upload_id}` | `{upload, parts: [{number, size, etag}]}` (ListParts; resume) |
| `POST .../uploads/{mount_upload_id}/parts/{part_number}` | -> `{part_number, size, url, expires_at}` (presigned PUT, 3600 s); 429 at `byos.upload_parts_per_hour` |
| `POST .../uploads/{mount_upload_id}/complete` | `{parts?: [{number, etag}]}` (`{}` is fine) -> `{key, size}`; 409 `UPLOAD_INCOMPLETE` |
| `DELETE .../uploads/{mount_upload_id}` | 204, abort |
| `POST /storage/mounts/{mount_id}/folders` | `{key: "a/b/"}` -> 201 `{key}` (a zero-byte marker) |
| `POST /storage/mounts/{mount_id}/move` | `{from, to}` (object to object, folder to folder) -> `{count}`; 409 `TARGET_EXISTS`, `OBJECT_TOO_LARGE` (over 5 GiB); 400 `FOLDER_TOO_LARGE` (over 500 entries), `MOVE_INTO_ITSELF`, `MOVE_SAME`, `KIND_MISMATCH` |
| `DELETE /storage/mounts/{mount_id}/objects/{key}` | a trailing `/` deletes a folder -> `{count}`; 409 `FOLDER_TOO_LARGE` |
| `POST /admin/storage/mounts/{platform_mount_id}/attach` and `/detach` | `{guild_id}` -> `{guild_id}` / 204 |
| `GET /admin/storage/mounts/{platform_mount_id}/attachments` | `[{guild_id}]` |

## Decisions

- Empty files (size 0) are refused at begin (a zero-byte multipart part is not portable); a folder is made only through the folder route.
- Rename and move are one call (`/move`); a move never overwrites (409 `TARGET_EXISTS`); it copies first and deletes after, so a
  failed copy loses nothing.
- Move and delete need `manage` on a server shelf (an uploader cannot delete others' files); on a member's own mount the owner can.
- The browser may send its etags at completion; the server completes from ListParts (the truth) and only checks them.
- Delete "with the catalog row": the catalog tables belong to byos-catalog (migration 1450). byos-catalog must call its own
  cleanup from `objects::delete_objects` and `move_objects` once `storage_objects` exists. Not done here: there is no table to
  write to yet.
- NFC normalisation of keys is still missing (needs `unicode-normalization`, a download).

## Still open

- Web done: `storage/browser/*` (mountUploads engine, UploadPanel with files / folder / drag and drop and a dock, NewFolder, RowActions for rename-move and delete, keys, writeErrors, browser.css), ledger store `mount` (IndexedDB v3, aborted at sign-out), `api.ts` write calls, `ObjectList.can_upload/can_manage` drive which controls show, fragment `p28-byos-browser-1.ts`. Commander items: Settings > Storage shows a Delisted badge and a note, test/enable are disabled for `delisted`, the policy block says to keep public listing off, the `MountObject.status` doc lists four statuses. `settings/user/storage/errors.ts` now reads the nested 409 code (it read only `body.code`, which the real server does not send). The UploadManager itself was not extended: its API is chapter-shaped, so the mount manager is a sibling reusing its constants, states and part PUT.
- Web tests: vitest src/storage (37), src/review + src/lounge/msg + src/settings/user (308 together), eslint, prettier, tsc, i18n-audit 0, check-css-tokens clean, vite build + bundle budget ok.
- Live `storage.yml` Garage case: upload 40 MiB, reload, resume, `ListParts` equals the ledger (CI only; no Garage on Windows).
- `lib/uploadLedger.ts` `mount` kind and `review/uploadManager.ts` reuse (web).
- OpenAPI / api-client / MSW regeneration (once at the end, or left to the integrator).
- An Admin button for attach/detach (byos-legal's `apps/web/src/admin/storage/*`).
- Quota note: `byos.upload_parts_per_hour` counts part URLs per member, not per mount.

## Tests run

- `cargo test -p sr-admin` (62), `-p sr-storage --lib` (115) and `--test s3_multipart_mock` (11: folder marker, copy, delete, list,
  virtual-hosted, pinned without DNS), `-p sr-api --lib storage::` (15: 7 write-side endpoint tests over the in-memory ledger
  and bucket, the rebind and NetGuard tests, the wave-27 mount tests), `-p sr-api --test route_matrix` and `--test
  openapi_names`, `-p sr-server --lib upload_jobs` (2).
- Postgres (scratch DB, `--include-ignored --test-threads=1`): `-p sr-db --test mount_uploads_repo` (1), `--test mount_attachments`
  (1), `-p sr-server --test authz_matrix storage_writes_hold_over_http` and `storage_scopes_hold_over_http` (2),
  `every_route_enforces_its_authz_row` with `SR_AUTHZ_GROUPS=5` (only the seven pre-existing failures) and `=1` (green).
- Not run yet: clippy `-D warnings` over the touched crates, the full sr-api / sr-server suites, web gates, `storage.yml`.
