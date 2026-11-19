# Handoff: wave 109, builder r2 (D-1029: the R2 test spec and runbook)

## Clock
Started 21:40 (machine clock); spec and runbook written; prettier and e2e `tsc` run 21:45 to 21:46. No job (the spec cannot run in a cell).

## Done
- `e2e/tests/r2-byos.spec.ts` (new), `docs/dev/r2-byos-test.md` (new), this handoff. Commit subject: `D-1029: the R2 test spec and runbook (UNRUN: needs the owner's bucket)`.

## Not done
- Nothing was run against anything: the spec is UNRUN by design, so its first run is the owner's and the commander's. Not run: vitest, eslint, i18n audit, doc-link check (left to the commander).

## What I read of the product
- **Where uploads go:** the bucket browser (`/storage/<mountId>`: `apps/web/src/storage/StorageBrowser.tsx`, `browser/UploadPanel.tsx`, hidden input `data-testid="sto-pick-files"`) -> `browser/mountUploads.ts` -> `POST /storage/mounts/{id}/uploads` -> `POST .../parts/{n}` (a presigned PUT: the browser sends each part straight to the bucket) -> `POST .../complete`. Keys are mount-relative; the server joins the mount prefix (`Mount::bucket_key`, `crates/sr-storage/src/mounts.rs`).
- **Always multipart:** the header of `crates/sr-api/src/routes/storage/uploads.rs`: every upload is a multipart upload in 16 MiB parts, even a 1 KB one (one part). The 60 MB file is four parts; there is no single-PUT path.
- **Message attachments do not use the member's mount:** `crates/sr-api/src/routes/messages/attachments.rs` has no mount; slots are signed against the instance storage (D-1026 says a cell cannot seed them for that reason). Step 2 reports this and tests the mount's image preview and download link instead. It is not testable with the owner's bucket.
- **Shelves:** `POST /storage/mounts/{id}/index` queues the `storage.index` job; `GET /shelves/music?mount=` lists; an audio file outside the media folders is classified by extension (`crates/sr-storage/src/index/classify.rs`), so a generated WAV at the mount root becomes a track. This needs the server's job runner: if the cell's jobs do not run, step 3 FAILs with "the index run did not finish in 120 s" (a finding about the stack, not the bucket). A Study file is not tested (the readers use the same presign; no PDF or EPUB is made).
- **CORS the code asks for** (`crates/sr-storage/src/s3admin.rs` constants, `cors_check.rs`): origin named (a wildcard does not count); methods PUT, GET, HEAD; allowed headers content-type and range; exposed ETag, Content-Range, Accept-Ranges, Content-Length. The docs have no JSON, only the sentence in `manual_steps`, so the runbook gives it as R2's dashboard JSON, built from the constants.
- **The stored prefix is exactly what was typed** (`key_prefix` in `routes/storage/mounts.rs` only trims leading slashes), so `sideroom-test` and `sideroom-test/` are two strings that behave the same; the spec requires the second, as the task says.

## The steps and what each proves
The runbook has the table. In short: 1 upload and CORS (PUT count, answer statuses, presence of `access-control-allow-origin`, "CORS: the bucket does not allow this origin"); 2 image shows, download links answer 200 with the right length; 3 shelf item lists, opens, ranged GET 206 with 1024 bytes, `content-range` and `accept-ranges` readable by the page, 206 inside the 60 MB object; 4 listing (once each, mount-relative, `../` refused); 5 delete through the Delete button and its confirm, list, signed URL no longer 200, shelf row gone; finally: delete what is left of this run's exact names, then two `R2 RESULT` lines.
Guards: skips unless `SR_R2_ON=1`; one `R2 STOP:` line and nothing touched if no mount is labelled `sideroom-test` (or two are), the prefix is not exactly `sideroom-test/`, or the mount is not active or its CORS check has not passed. Trace, screenshots and video are off. It logs no URL, host, key or header value. It deletes only exact names in its own `made` set, never by pattern, and never edits or removes the mount. About 65 MB uploaded in all (asserted under 100 MB).

## What it cannot prove
Channel attachments; Study readers; video playback and transcoding; a friend's or a server's shelf of the bucket; the full download of the 60 MB file (only a range); another provider; that nothing was written outside the prefix (the app cannot list there: only the `../` refusal and the owner's dashboard check show it); the sign-in form (it signs in by the API as the e2e Owner).

## Product findings that look wrong for a real bucket
1. **`crates/sr-storage/src/cors_check.rs`, `check` and `unsupported` (about lines 196 to 213):** only 501, 405 and NotImplemented fall back to the live preflight. A 403 AccessDenied from `GetBucketCors` (what R2 may answer to an object-only, bucket-scoped token; I could not verify this) becomes a 409 "The service refused to read or write the bucket's CORS rules" (`crates/sr-api/src/routes/storage/mount_ops.rs`, lines 205 to 211), `cors_ok_at` stays empty, and every upload and presign answers `CORS_NOT_READY` although the bucket's CORS is right. Falling back to `manual_or_verified` on a 403 would pass here. The runbook warns the owner and offers an Admin Read and Write token for this one bucket as the owner's decision.
2. **`crates/sr-api/src/routes/storage/mounts.rs` lines 220 to 233:** `CreateMountBody` derives `Debug` and holds `secret_access_key: String` in plain text. Nothing prints it today (no `{:?}` of it and no `#[instrument]` in the storage routes), but a later `tracing::debug!(?body)` would log the secret. `Secret` (`sr-storage/src/mounts.rs` 174) is redacted; this body is not.
3. **`GET /users/@me/storage/mounts` returns `access_key_id` and the endpoint** (`MountObject`, `mounts.rs` about 160 to 215; an R2 endpoint holds the account id). The spec takes five other fields out of the answer and drops it; a trace of the page would keep it, which is why the spec turns tracing off. Not a fault for the owner's own list; noted for the commander's rule on what a run may keep.

## FINDERS (rule 13)
No app file changed; both files are new. `r2-byos` is named nowhere else: none.

## JOBS
None submitted.

## Checks (own tree)
- `npx prettier --check e2e/tests/r2-byos.spec.ts docs/dev/r2-byos-test.md`: passed, about 21:45.
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0, about 21:46.
- Not run: vitest, eslint, i18n audit, doc-link check (left to the commander). The spec itself: UNRUN.

## No credential appears in my files
`grep -n -i "secret\|access.key\|token" e2e/tests/r2-byos.spec.ts docs/dev/r2-byos-test.md` (plain grep, because the files were untracked when I ran it). Spec hits (lines 11, 14, 19, 49, 143, 172) are all comments: the spec holds no key; a presigned URL carries an access key id so none is printed; the owner makes the Cloudflare token by hand; a trace would keep the mounts list; a GET needs no CSRF token; the list answer carries an access key id the spec does not read. No spec line names, holds, reads or logs a key, a secret or a token value. Runbook hits: the lines that tell the owner to make, keep, never paste and revoke the Cloudflare API token, the "Check CORS" risk note, the server's-secret wording in the database row, and the "chat, files, shell history" row. No value, no endpoint with an account id, no bucket name.

## Closing text for my row and proposed new rows
- D-1029: spec and runbook written, UNRUN; the first run is the owner's and the commander's.
- Proposed rows: (a) the CORS check on R2 should fall back to the live preflight on a 403 (finding 1; to be confirmed by the owner's run); (b) redact `CreateMountBody`'s secret in `Debug` (finding 2).
- Changelog sentence: a spec and a runbook now exist for testing bring-your-own-storage against a real R2 bucket; nothing in the app changed.

## Owner questions (to Fable; defaults listed)
1. Is "Object Read and Write" scoped to one bucket enough for the app's CORS check? Default: try it; if the check is refused, the owner decides whether to use an Admin Read and Write token for this one bucket for the test.

## PROCESSES
`strays.sh p109-r2`: `strays: none found` (run at the end).
