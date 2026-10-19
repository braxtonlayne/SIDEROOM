# Handoff: feat/p16-uploads (phase 16)

Items: LNG-CMP-007, DON-UI-001, ADM-CNT-001. Migration used: `1030_attachment_multipart.sql`.

## Done

### LNG-CMP-007 resumable multipart uploads for chat attachments over 64 MB
- Storage layer reused: the library's `PartStore` / `S3Multipart` / `check_parts` (`sr-library::multipart`).
- `POST /channels/{id}/attachments`: for a file above `SizePolicy::multipart_threshold` (64 MB) on a server with multipart wiring, the slot has `upload_url: ""` and a `multipart` object (`part_size`, `part_count`, presigned part URLs, 6 h TTL). Smaller files and servers without S3 multipart (memory store, `SR_MEDIA_SINGLE_PUT=1`) keep the single presigned PUT.
- `GET /channels/{id}/attachments/{upload_id}/parts`: received parts and fresh URLs for the missing ones (resume).
- `DELETE /channels/{id}/attachments/{upload_id}`: discards an unattached upload (aborts the multipart upload / deletes the object).
- `complete` (and the implicit finalize when a message references the upload) assembles the object from the parts the store holds, then runs the normal verify/sniff. Idempotent.
- `attachment_multipart` table (migration 1030) holds upload id + part size per slot; `MessageStore` gained `set/get/clear_attachment_multipart` (memory and Postgres).
- The hourly abandoned-upload sweep (`attachments.gc_unreferenced`) now aborts the multipart upload before deleting the slot; `register_attachment_gc` takes the part store.
- Web (`lounge/msg/upload.ts`): parts sent 3 at a time, per-part retry with backoff, waits for the browser to be online, after a stall asks the server which parts arrived and sends only the rest (4 rounds), cancel aborts server-side. A localStorage record (`sr.upload.resume`, 20 h) lets sending the same file (name, size, lastModified) in the same channel after a reload continue the old upload.

### DON-UI-001 `/support`
`/support` is now `support/SupportPage.tsx` (route in `support/routes.tsx`): why donate, hosting bar (reused `HostingBar`), Patron card (price, perks, status), Server Levels explainer, invite credits, leaderboards link, then the old help answers (`HelpSections` in `routes/entrance/Static.tsx`). Read-only: paying stays in Settings > Support / Patron. `/support` is no longer a core (first-paint) path. Strings: `i18n/messages/en/p16-uploads-1.ts`.

### ADM-CNT-001
Audit: all five tabs already existed; ticked with a new `reviewHub.test.tsx` (tabs shown per permission).

## Tests run
- `cargo test -p sr-api --lib messages::tests::` (56 pass, incl. two new multipart tests), `--test route_matrix` (5 pass).
- `cargo test -p sr-db --test message_store -- --include-ignored --test-threads=1` on scratch DB `sr_p16_uploads` (pass, dropped afterwards).
- `cargo fmt --check`; `cargo clippy -j 2 -p sr-chat -p sr-db -p sr-api -p sr-server --all-targets -- -D warnings` clean.
- Web: tsc, eslint on touched files, prettier --check, vite build + bundle budget OK. vitest (lounge, support, admin, i18n, library, review): all pass except pre-existing environment failures below.
- OpenAPI, TS client and MSW mocks regenerated; two route-matrix rows added.

## Known environment failures (not caused by this branch)
- This install runs some vitest invocations with vitest 2.x, so `src/test/vitestVersion.test.ts` and `review/uploadManager.test.ts > retries a failing part...` fail (`rejects.toThrow` quirk). Same on base.
- `i18n/firstpaint.test.tsx > holds a non-entrance route...` fails on base as well (verified against integration/phase16).

## Still open / not done
- LNG-CMP-001 (Lexical composer) not touched, as instructed.
- No Playwright/e2e run (not allowed for this agent). A real S3 round trip of the chat multipart path was not run; the part store is the same `S3Multipart` the library uses (covered by `sr-storage` mock tests), the chat route is tested with `MemoryPartStore`.
- A per-user cap on simultaneous open multipart uploads is not implemented; abandoned ones are swept after 24 h.

## Decisions / owner questions
1. `/support` copy (why donate text, no-ads line, levels note) is a draft; the owner's wording is still awaited.
2. `/support` replaced the public help stub as the page at that URL (help answers are kept at its end). Signed-out visitors see a sign-in prompt for the donation parts. If the owner prefers the help page to stay at `/support` and the donation page elsewhere, it is a one-line route change.
3. Resume after reload needs the user to pick the same file again (the file bytes are not stored in the browser, unlike the Library upload manager's IndexedDB ledger). Storing the file in IndexedDB for chat attachments would need a storage-quota decision.
