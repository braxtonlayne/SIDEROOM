# Wave 63, cdn builder: SEC-AUD-157..160 (D-641, D-642, D-643)

Branch `fix/p63-cdn` (base `integration/phase62` at `a625b943`). No migration, no route or response shape change: `openapi.json` and the generated client are untouched. `docs/inventory/21-feature-phase.md` is not edited. No UI.

## Done (all four items)

Commits (see `git log`): `SEC-AUD-157..159` (one commit for items 1 to 3), `SEC-AUD-160` (item 4), a small fix commit for the artwork test, then this handoff.

### SEC-AUD-157 (D-641, D-642): one table, and the shelf object URL
- `response_for(key)` moved from `cdn/mount.rs` to `cdn/content.rs` as `pub(crate) fn response_for(key: &str) -> (&'static str, String)`, same behaviour. `cdn` is already `pub mod` and `content` is `pub mod`, so no visibility change was needed. `mount.rs` calls `content::response_for`.
- `from_extension`: `m4b` maps to `audio/mp4`. The table's unit test asserts `m4b` and that `mkv`, `avi`, `ts`, `wma`, `heic`, `bmp`, `ico`, `srt` give `None` (with a comment pointing at D-642). New unit test `response_for_follows_the_table`.
- `shelf_access::presign_object_url` signs with `presign_get_as(&mount.bucket_key(path), ttl, &response_for(path))`. Errors still go through `bucket_error`; there is no fallback to a plain `presign` (`PresignUnsupported` is an error). Signature and callers unchanged (`shelf_object_url`, the profile showcase).

### SEC-AUD-158 (D-641): the storage presign route and its delegates
- `objects::presign`: PUT keeps `storage.presign(PresignMethod::Put, ..)` exactly; GET signs with `presign_get_as` and `response_for(&body.key)`. Method check, `safe_key`, `is_save_key`, range check, `reach`, `key_allowed`, CORS gate, TTL rules, `log_staff` and the response are untouched.
- The three delegates (`music/tracks.rs`, `voice/jukebox.rs`, `guilds/scrapbook.rs`) each build a `PresignBody` with `method: "GET"`: confirmed, no edit.
- `objects.rs`'s own `content_type()` (the metadata view) is NOT the response table and was left alone. Do not merge the two.
- No PUT test: the `uploads_tests.rs` harness has no emulator-save PUT presign fixture (`emu/saves/..`), so `a_presigned_put_for_a_save_is_unchanged` was not built (as the task said). The PUT branch is textually the old call.

### SEC-AUD-159 (D-641): shelf artwork
- `Signer::url` signs with `presign_get_as` and `response_for(key)`; a failure is `None` as before.
- `grep -rn "PresignMethod::Get" crates/sr-api/src`: no hit left. Hits elsewhere in `crates/`, all staying: `sr-media/src/mount_job.rs:229` (the server's own worker reads the object), `sr-preflight/src/checks.rs:237` (preflight), `sr-storage/src/storage.rs` (the implementation and its unit tests), `sr-storage/tests/s3_live.rs`, `s3_mount_live.rs`, `s3_scrapbook_live.rs` (live tests). None is a link given to a client.

### SEC-AUD-160 (D-643): the connection test checks the overrides
- `S3Admin::get_object_as(key, content_type, disposition, timeout)` (same signed GET with `response-content-type` and `response-content-disposition` in the query).
- `probe.rs`: seventh row `response_type`, right after `put`. `put_row` now takes `&mut Option<ProbeRow>` and runs the new GET between the read-back and the delete (the probe object exists then). Pass: 2xx, `content-type` is `image/png` (case-insensitive, parameters ignored) and `content-disposition` starts with `attachment`. Fail: 2xx with other headers ("ignores the response type" sentence, "It cannot be used."); the S3 reason on a refusal; "no answer" on a transport error; `timed_out("response_type")` when the budget is spent; a "Skipped" Fail when the `put` step could not write or read back. `all_timed_out`, the dead-endpoint skip list, the module comment and the `ProbeRow::id` doc list seven rows. Messages carry no URL, secret or body.
- Fixtures: `s3_probe_mock.rs` (`fake_s3_with(handler, honour)`; `fake_s3` honours) and `fake_s3` in `mounts_tests.rs` answer a GET of the probe object that carries `response-content-type` with `content-type` and `content-disposition` from the query.

What a `failing` mount stops: `status_after_test` (`mount_ops.rs:152`) turns a failed test into `failing` (a `disabled` mount is never moved). **Finding: `failing` stops nothing.** `scopes.rs` has the test `a_failing_mount_is_still_readable` ("a health flag for the owner's screen, not a refusal"); `sr-db/src/mounts_repo.rs` `readable()` and `sr-server/src/storage_jobs.rs:433` treat `Active | Failing` alike; `MountResolver::resolve` refuses only `Disabled` (and `Delisted` for serving). So a mount whose provider ignores the overrides is shown as failing with the sentence "It cannot be used", but `/cdn/mount`, presigns and the storage jobs keep working on it. Whether `failing` should refuse serving is a product decision (owner question 1). Mounts tested before this change are not tested again until someone presses Test (or the recheck job, which only re-stamps CORS): they keep their old six-row result and status.

Web, not mine, flag: `apps/web/src/settings/user/storage/types.ts:12` and `mocks.ts:40-45` list six ids; the UI should learn `response_type` (and its row label) or show the raw id/message. The connection-test API returns the extra row without a shape change (rows are a list of `{id, status, message}`).

## Not done
- A live check against real providers (no network). The live test `s3_mounts_live.rs::the_connection_test_passes_on_garage` still passes by construction on a service that honours the overrides (it asserts no row fails); not run.
- The UI strings for the new row id (not my files).

## Tests
Heavy lock, `-j 4`, own cold target:
- `cargo test -p sr-storage --test s3_probe_mock`: 14 passed, including new `a_service_that_honours_the_overrides_passes_response_type` (also asserts the GET happens before the delete and the row follows `put`), `a_service_that_ignores_the_overrides_fails_the_test` (`t.failed()`, only `response_type` fails), `a_refused_response_type_read_and_a_failed_write_say_why`; existing row lists and `len() == 7` updated, cors row index 4 to 5.
- `cargo test -p sr-api --lib -- cdn:: storage::uploads_tests storage::mounts_tests`: 96 passed. New: `a_presigned_get_for_a_document_is_a_download` (xlsx and html keys), `a_presigned_get_for_audio_keeps_its_type`, `a_shelf_object_url_for_an_html_file_is_signed_as_a_download`, `a_shelf_object_url_for_a_video_keeps_its_media_type`, `shelf_artwork_urls_carry_the_response_type`, content.rs `response_for_follows_the_table` and the extended extension test. The shelf-URL endpoint had no test: the harness in `uploads_tests.rs` gained a stub `ShelfAccessStore` (`StubShelves`, only `catalog_object` is real). URL queries are parsed with `url::Url`, not matched as strings. `mounts_tests` id list has `response_type`; the existing passing-connection-test tests pass with the updated `fake_s3`.
- Watched fail without the change: see the "Watched failing" section at the bottom (filled from the second lock round).
- `cargo clippy -j 4 -p sr-api -p sr-storage --all-targets -- -D warnings`: see the bottom. `rustfmt` run on every file I changed.

## Proposed inventory rows (after SEC-AUD-153)
- **SEC-AUD-157** [BE] One response table for every link SIDE ROOM signs: `response_for` lives in `routes/cdn/content.rs`; `.m4b` plays as `audio/mp4`; `GET /shelves/{id}/objects/{hash}/url` (and the profile showcase through it) is signed so the bucket answers non-media as an `application/octet-stream` download. D-641, D-642. Handoff `fix-p63-cdn.md`.
- **SEC-AUD-158** [BE] `POST /storage/mounts/{id}/presign` signs a GET with the same response table (a copied or opened storage link downloads a document instead of opening it as a page; media keeps its type); its three delegates (music tracks, jukebox, scrapbook) follow. A PUT (emulator save) is unchanged. D-641.
- **SEC-AUD-159** [BE] Shelf artwork URLs are signed with the response table. After this no `PresignMethod::Get` is left in `crates/sr-api`. D-641.
- **SEC-AUD-160** [BE] The storage connection test has a seventh row `response_type`: it asks the bucket for `image/png` and an attachment and fails the mount when the service ignores the override. D-643.

## Proposed threat-model text
D-629 row, residual: replace "a provider that ignores the overrides" with: "A provider that ignores the response overrides is detected by the connection test (D-643: the `response_type` row fails and the mount shows `failing`). What remains: a mount not tested since D-643 (the test runs only when someone presses Test), and the fact that `failing` is a health flag that does not stop serving (open question). Objects in a bucket outside the app keep whatever type their owner stored; SIDE ROOM links never depend on it."

## Changelog sentence (members)
"Opening or copying the link of a file in your storage that is not an image, video or audio (a web page, a document) now downloads it instead of opening it in the browser; audiobooks (`.m4b`) now play."

## Owner questions
1. Should a `failing` mount refuse serving (links, presigns, jobs) when the failing row is `response_type`? Today `failing` is only a flag; a provider that ignores the overrides stays usable. Default taken: no change (flag only).
2. The six-id lists in the web (`types.ts`, `mocks.ts`) need the new id and a label ("Response type"): next wave's web builder.
3. D-642 keeps `.mkv`, `.avi`, `.ts`, `.wma`, `.heic`, `.bmp`, `.ico`, `.srt` off the inline list; a copied link to one now downloads. No change made.

## Watched failing, clippy, cleanup
- With the three signers (`shelf_access.rs`, `objects.rs`, `shelves.rs`) put back to the base version, the five new endpoint tests all FAILED (0 passed, 5 failed); with the change they pass (5 passed).
- With the `response_type` pass condition forced to true, `a_service_that_ignores_the_overrides_fails_the_test` FAILED; restored, 14 of 14 pass.
- `cargo clippy -j 4 -p sr-api -p sr-storage --all-targets -- -D warnings`: clean (second round; the first caught a wrong `Reach` type in my artwork test, fixed). `rustfmt --check` clean on my files.
- No Postgres scratch DB was created (all tests are in-memory). `p63-cdn-target` is deleted.
