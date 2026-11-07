# Handoff: fix/p53-storage (wave 53 storage: shares, holds, takedowns, archives)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p53-storage`, branch `fix/p53-storage` (base main `491aad85`). Not pushed.
Migration used: `2390` only. No UI text or screen changed, so no visual sweep (see "Not done").

## Done

### SEC-AUD-005: "companion" files are an allow-list
- `sr-storage/src/scopes.rs`: new `is_companion(key)`: artwork (jpg/jpeg/png/webp/gif/avif), subtitles
  (srt/vtt/ass/ssa/sub/idx), the sidecars the shelves read (nfo, cue, m3u, lrc, `ComicInfo.xml`, `series.json`, the
  indexer's `.sideroom.json`) and folder markers (`x/`). A bare `.png` (a dot file) is not artwork. `Access` gains
  `others: bool` (a guild role with `manage`); `Access::permits` now answers: shelf kind -> the share or grant; companion
  -> any share or view grant, as before; anything else -> owner, staff (`unrestricted`) or `manage` only.
- Every reader goes through `Access::permits` (listing filter `objects.rs:434`, presign/meta `key_allowed`, shelf readers
  `shelf_access.rs:345`, `shelves.rs:677`, scrapbook `scrapbook.rs:605`, `resolve`), so a listing never shows what the
  presign refuses. Search/index rows are media only (classify) and pass through the same check.
- Tests: `scopes.rs` `companions_are_an_allow_list`, `a_share_reads_companions_but_not_other_files` (friend, owner,
  staff, guild view grants vs `manage`); route test `uploads_tests.rs` `a_share_reads_companions_but_not_other_files`
  with a fake S3 listing: a music-only friend lists `cover.jpg`, `song.mp3`, `track.lrc` and not `Taxes/2024.xlsx`,
  `notes.txt`, `film.mkv`; presign 404 for those, 200 for the companions; owner 200; guild viewer 403 / manager 200 on
  `Ledger.xlsx`.

### SEC-AUD-006: held evidence survives a channel or DM delete
- Choice: **refuse the server-channel delete; keep a group DM's rows when its last member leaves**, plus a database
  backstop. Why: D-122 (held messages are never purged from staff view) and D-212 (holds never expire by themselves)
  mean the rows must stay; D-213 says only platform staff place or release holds, so a server admin must not be able to
  undo one by deleting the channel, and a soft-delete would need a new hidden-channel state through every channel
  reader. Refusing matches the precedent of migration 1120 (held library/cinema rows: "Release the hold first"). For a
  group DM the action is a member leaving, which must never be blocked, so the leaver still leaves and the channel row
  stays with no recipients (nobody sees it); its held rows stay for staff.
- `2390_held_messages_survive_deletes.sql`: `BEFORE DELETE` triggers on `messages` and `attachments` with the existing
  `moderation_refuse_held_delete()` (SQLSTATE 23001), so any path (cascade included) that reaches a held row fails.
- `ChannelRepository::holds_evidence` (Pg: through the partial hold indexes, the channel and its threads; memory repos
  too). `GuildAdmin delete_channel` (`sr-chat/src/admin_service.rs`, reached by the REST route, the bridge and feature
  actions) answers 400/50035 `CHANNEL_UNDER_MODERATION_HOLD` with a plain sentence. `PgSocialRepo::delete_dm` checks in
  its transaction and drops the recipients instead of the channel when held.
- Tests (Postgres): `sr-db/tests/held_channel_delete.rs`: a held message and (in another channel) a held attachment on
  an unheld message both refuse the delete with the code; a raw `DELETE` of the channel or the message is refused by
  the trigger; after release both delete. The last member leaving a group with a held message leaves it with no
  recipients and the message kept; an unheld group is deleted as before.

### Verify-then-fix items
1. **Held attachment object deleted between purge selection and hold re-check (server-1 HIGH 2): real, fixed.** All
   three jobs (`purge_deleted_messages`, `purge_removed_attachments`, `expire_large_attachments` in
   `sr-server/src/jobs.rs`) now lock and re-check their rows (`FOR UPDATE SKIP LOCKED`; the expiry takes `FOR SHARE` of
   the message) inside the bounded batch transaction before any `remove_object`, and finish the row change in the same
   transaction. A hold placed in the window waits for the batch and then finds the row gone (or expired). Test
   `sr-server/src/jobs_hold_race_pg_tests.rs`: a bucket whose `delete` tries to place the hold from another connection
   (300 ms lock timeout) exactly in the window; before the fix the hold landed and the held row lost its file.
2. **Delisted whole mount still presigned via `/cdn/mount`: real, fixed.** `MountStorages::for_serving` refuses
   `Delisted` as well as `Disabled`; the `/cdn/mount` redirect and ring playback use it (index and removal jobs keep
   `for_mount`). Test `cdn/mount_tests.rs` `a_delisted_mount_stops_serving_links_minted_before_the_takedown`.
   Also found: a single-object legal takedown (`storage_object` item) was never blocked on `/cdn/mount` either
   (`takedowns.rs` "never blocked on /cdn"), though that route already consults the blocklist under
   `mount/<id>/<key>`. Disabling the item now adds that block; the notice's restore lifts it with the other blocks.
   `sr-moderation/tests/storage_takedown.rs` updated to assert the block and its removal.
3. **Takedowns keyed by path stop applying after a move or rename: real, fixed for moves.** `POST
   /storage/mounts/{id}/move` refuses (409 `OBJECT_TAKEN_DOWN`) when any file it would move (alone or in a folder) is
   delisted. Test `uploads_tests.rs` `a_taken_down_file_cannot_be_moved_or_renamed`. Not fixed: the same bytes
   uploaded again under a new name (see owner questions).
4. **Zip central directory parsed before the 4000-entry cap: real, fixed.** `ingest/archive.rs`
   `check_zip_directory` reads only the tail: every end-of-central-directory candidate (the zip crate falls back to the
   next one back), its ZIP64 locator and record when present; refuses more than `max_entries` entries
   (`TooManyEntries`) or a directory over 4 KiB per permitted entry (`TooLarge("archive directory")`). Called before
   the zip crate in `open_zip` and `epub::inspect`. Tests: `too_many_entries_are_refused_from_the_tail_before_the_directory_is_parsed`
   (4001 entries: refused after reading < 100 KB of a ~380 KB file, for CBZ and EPUB; before the fix the whole
   directory was read) and `zip64_and_planted_directory_records_are_checked`.
5. **Mount-transcode SSRF via redirects: real, fixed.** `sr-transcoder/src/http_client.rs`: the worker's client
   follows a redirect only when the server itself answered it (the signed `/cdn/...` paths), at most three; a redirect
   from a bucket is not followed and the download fails with its status. `sr-media/src/mount_job.rs`: the member
   endpoint is re-vetted against the address policy at every presign (GET at claim, PUT), not only at queue time.
   Test `only_the_servers_own_redirects_are_followed` (a 307 from the "bucket" to the worker server's own address is
   not followed; a 307 from the server's base is).

## Not done
- **Comment-reply cascade (`db-migrations.md` MED 2)**: not a small fix. Keeping B's reply when A deletes the parent
  needs a tombstone ("comment deleted") row state that the Cinema and Library comment readers and their UI render;
  switching the FK to `SET NULL` would instead lift replies to top level out of context. Left for a slice with UI.
- **Web wording for `OBJECT_TAKEN_DOWN`**: the bucket browser shows its generic error for the new 409
  (`storage/browser/writeErrors.ts` has no entry). Adding a sentence is UI text and needs the visual check; the case
  is rare (the owner moving their own taken-down file), so it is left for the web builder.
- Re-uploading taken-down bytes under a new name still bypasses a path-keyed takedown (needs hash keying).

## Tests run
All green, scratch DB `sr_p53_storage` (dropped afterwards):
- `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-storage -p sr-chat -p sr-db -p sr-library -p sr-media
  -p sr-moderation -p sr-transcoder -p sr-api -p sr-server --all-targets -- -D warnings`.
- `cargo test` of sr-storage, sr-library, sr-transcoder, sr-chat, sr-media (whole crates), sr-api `--lib` (823),
  sr-server `--lib` (136) and its `jobs` Postgres tests `--ignored` (14, the new race tests included).
- Postgres `--ignored`: sr-db `held_channel_delete`, `guild_admin_repo`, `social_repo`, `thread_repo`,
  `scopes_repo`, `shelf_access`, `storage_legal`, `data_requests`, `hold_guard`, `review_repo`, `self_delete`,
  `chat_repo`, `message_store`; sr-moderation `storage_takedown`, `takedowns`, `db`, `deadline_windows`, `rings`.
- Fails without the fix, run: the companion test (old rule restored in place: `a_share_reads_companions_but_not_other_files`
  fails) and the ZIP test (pre-check removed: `too_many_entries_are_refused...` fails). The others were reasoned from the
  code, not re-run against the old code: the hold race (the hold lands, the held row loses its file), the CDN delist
  (302 instead of 404), the move (200), the redirect (followed, Ok(1000)), the channel delete (cascade succeeds).
- OpenAPI: only the move route's 409 description changed; `openapi.json` and `schema.d.ts` edited and the
  `sr-server openapi` output diffed equal. No route added, so `route_matrix.tsv` and the authz matrix are unchanged.
- No web code touched: no tsc/eslint/vitest/visual sweep needed.

Commit subjects: the task numbers only 005 and 006; the four verify-then-fix commits carry `SEC-AUD-006` for want of
their own ids (rename at integration if they get rows).

## Owner questions
1. Whole-shelf grants (STU-ZK-001, "read every kind") now read media and companions but not "other" files; only
   `manage`, the owner and staff do. Default kept strict (Discord has no analogue; least privilege). Should a
   whole-shelf grant also read every other file?
2. A server channel holding held evidence cannot be deleted by the server's admins until staff release the hold
   (refusal with a sentence that says to contact staff). Alternative: hide the channel instead (a new channel state).
   Default: refuse, as for held library/cinema rows.
3. A group DM whose last member leaves while it holds held messages stays as a recipient-less channel until staff
   release the hold; nothing deletes it afterwards. Should the hold release (or a sweep) delete such empty groups?
4. Should a storage-object takedown follow the bytes (object hash) so a re-upload under a new name is caught, as the
   staff delist (`shelf_objects.delisted_at`) already does?
