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

## Known environment failures
- Resolved in the follow-up pass: after `pnpm install --offline --frozen-lockfile` vitest is 3.2.7 and `vitestVersion.test.ts` / `uploadManager.test.ts` pass. `i18n/firstpaint.test.tsx > holds a non-entrance route...` was not a code bug: after `vi.resetModules()` the lazy Lounge route chunk is transformed cold (~9 s here) and the 5 s `findByRole` default expired; the test now waits up to 20 s.

## Still open / not done
- LNG-CMP-001 (Lexical composer) not touched, as instructed.
- No Playwright/e2e run (not allowed for this agent). A real S3 round trip of the chat multipart path was not run; the part store is the same `S3Multipart` the library uses (covered by `sr-storage` mock tests), the chat route is tested with `MemoryPartStore`.
- A per-user cap on simultaneous open multipart uploads is not implemented; abandoned ones are swept after 24 h.

## Decisions / owner questions
1. `/support` copy (why donate text, no-ads line, levels note) is a draft; the owner's wording is still awaited (owner item 12, open-questions.md).
2. Page placement: answered by D-270 (`/support` stays the donation page, the help answers moved to `/help`); done in L-30.
3. Resume after reload without picking the file again: answered by D-271 (IndexedDB ledger, 5 rows, 20 h, localStorage fallback); done in L-31.

## Follow-ups (L-30, L-31, L-32)
- **L-30** `/help` is an Entrance static page (`Help`, `HelpSections`), signed-out, in `i18n/corePaths.ts`; `/support` drops the help block and ends with "Need help? Read the help page". Library footer "Help" and the header "?" menu (new `lounge.helpMenu.help`, always shown) link `/help`; "Support SIDE ROOM" and the voice refusal links stay on `/support`. Strings in `i18n/messages/en/p16-uploads-1.ts`.
- **L-31** Shared ledger `lib/uploadLedger.ts` (db `sideroom-uploads` v2, stores `uploads` and `chat`; the Library manager re-exports its ledger classes unchanged). Chat multipart slots write a row (`id` = upload id, channel, sizes, parts `done`, `savedAt`, the File, plus optional `maxSize`/`description`); `done` is updated per confirmed part. Rules in `ChatUploads`: cap 5 (oldest dropped, slot aborted), 20 h expiry checked on open and every write, removal on complete / cancel / 404 (a 403 also aborts the slot). If the ledger cannot store the row (`put` false: quota, blocked, no IndexedDB) the localStorage `sr.upload.resume` record is written as before. On composer mount `reattachUploads` puts the channel's rows in the tray as `uploading` files and continues from `GET .../parts`; sending is held while they run; removing one calls `discardChatUpload` (DELETE + row removed); leaving the channel pauses them and keeps the rows. A finished file carries `uploaded` so `uploadFiles` does not upload it again. Sign-out: `clearUploadsOnSignOut()` (keepalive DELETE of open slots, clears both stores, 2 s cap) runs in `settings/user/api.ts` `logout`/`logoutAll` before the session ends. No `navigator.storage.persist()`.
- **L-32** Docs updated: specs (lounge-chat, entry-landing-and-login), doc 05, inventories 02, 05, 11.
- Tests: `lib/uploadLedger.test.ts` (new), `lounge/__tests__/uploadMultipart.test.ts` (ledger row, fallback, cancel, reattach, 404, other channel, no re-upload). Not covered: the real IndexedDB wrapper (no fake-indexeddb, no new dependencies allowed) and a Composer-level reload test; a real-browser check belongs to the e2e agent.
- Open: sign-out via an expired session (401) does not run the clean-up; rows then age out after 20 h.
