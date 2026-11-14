# Wave 68 staff (SEC-AUD-213..217; D-692, D-694, D-695, D-705)

Branch `fix/p68-staff` (base: the commander's `integration/phase68`). One Rust migration (2930, an index), one new answer-type pair, no new route. `openapi.json` and the generated client are regenerated here (the only builder who does).

## Done (commits, in order)

See `git log fix/p68-staff` (subjects start with the SEC-AUD id). Summary by item:

1. **SEC-AUD-213, the review queue names people (D-692).** `GET /admin/review/queue` now answers `ReviewQueuePage` (the `SubmissionPage` fields flattened, plus `users`) and `GET /admin/review/submissions/{id}` answers `ReviewSubmissionDetail` (the `SubmissionDetail` fields flattened, plus `users`). `ReviewUser` is `{id, username, global_name, avatar}`, built from the `UserSummary` the chat repository's `UserLookup` already returns (the shape of `routes/settings/deps.rs` and `routes/developer`): no email, no flags, no new user object. One `UserLookup::users` call per answer: the ids are the submitters and claimers of every row and the `submitter` filter's id. A deleted account (null id, or an id with no account) is simply not in `users`. A lookup failure is logged and leaves `users` empty (the page falls back to the id). `ReviewDeps` gained `users: Option<Arc<dyn UserLookup>>` with `with_users` (wired in `sr-server/src/app.rs`, 3 lines, the only line of that file I touched). The member routes and `SubmissionPage` are unchanged. Web: `admin/sections/Review.tsx` shows `@username` on the row (display name in the `title`, the id when `users` lacks it), names the claimer in the "claimed by" text, the detail ("@name (Display Name) id", the id in the mono style), the "held by" line and the take-over dialog, the timeline actors, and the chip reads "Submitted by @username" (also when the filter matches nothing: the server sends the filter's user). The answers of claim, release and the decisions carry no `users`, so the detail keeps the people of its last GET.
2. **SEC-AUD-214, subtitle tracks add their language (D-694).** See "Where the language function lives".
3. **SEC-AUD-215, the takedown prefix (D-695).** `block_asset` adds `subtitles/{asset}/`. See "What the takedown test showed": the line alone did nothing on the CDN, so `routes/cdn/serve.rs` also asks the blocklist about the mapped key.
4. **SEC-AUD-216, workers show GPU and throughput (D-705). Closes ADM-CNT-006.** `WorkerCapabilities.gpu: Option<String>` (`#[serde(default)]`), cleaned by `WorkerService::register` (`clean_gpu_name`: control and direction-changing characters dropped, trimmed, 80 characters, empty is none). `WorkerView` gained `speed_now`, `jobs_done_24h`, `jobs_failed_24h`, computed in the one statement that lists the workers (three correlated subselects). Migration 2930 adds an index. The worker fills `gpu` from `nvidia-smi` (below). Web: `admin/cinema/TranscodingSection.tsx` (`WorkerGear`, a second meta line, so nothing wraps badly at 375 px: the first line keeps version, jobs, last seen and "N at once").
5. **SEC-AUD-217, dead keys.** Removed `admin.channelEditor.logoNotAvailable` and `...CallSign` (`en-staff/admin-a-1.ts`; the comments of both `p67-media-1.ts` fragments updated), and the two keys item 1 stopped using (`admin.review.heldByOther`, `admin.review.takeOverDescription`, `en-staff/admin-b-4.ts`; replaced by `...Named` keys in `en/p68-staff-1.ts`). Each was grepped in `apps/web/src`, `e2e`, `scripts` first. **`settings.notifications.digest` was NOT removed:** no key list reached me from the commander and on this branch `settings/user/sections/Notifications.tsx:266` still names it (the presence worktree has stopped naming it, uncommitted). It goes to the next wave's list below. `node scripts/staff-keys.mjs --check` fails only on my six new keys in `en/p68-staff-1.ts` (staff-only, the commander's `--write` moves them: `admin.review.submitterTitle`, `...heldByOtherNamed`, `...takeOverDescriptionNamed`, `admin.transcoding.workers.noHardware`, `...speedNow`, `...last24h`).

## Not done

- Nothing of the five items is dropped. Not run locally (left to CI, per the brief): the whole-crate and workspace suites, the route matrix (no route changed; the two answer types are named in `openapi.json` only), the bundle/entrance/staff-delivery checks (nothing first-paint or public changed; `Review.tsx`, `TranscodingSection.tsx` and `en/p68-staff-1.ts` are admin or fragment files).
- No backfill of titles that already have subtitle tracks (D-694, as decided).
- A title editor open in another tab shows the old language list until reload (no refresh built, as asked).

## The answer types I changed

- `GET /admin/review/queue`: `SubmissionPage` becomes `ReviewQueuePage` = `{items, total, limit, offset, users}`.
- `GET /admin/review/submissions/{id}`: `SubmissionDetail` becomes `ReviewSubmissionDetail` = `{submission, items, events, kind_label, users}`.
- New schema `ReviewUser`. Operation ids and paths are unchanged, so `route_matrix.tsv` and `authz_matrix.rs` needed no edit.
- `WorkerCapabilities` (+ `gpu`) and `WorkerView` (+ `speed_now`, `jobs_done_24h`, `jobs_failed_24h`) in the workers list answer. The worker registration body gains the optional `gpu` (plain axum, not in `openapi.json`).
- The audit entries `cinema.subtitle.upload` and `cinema.subtitle.update` carry `language_added` (the language, or null).

## Where the language function lives, and why (item 2)

SQL in `sr-db`: `media_repo/languages.rs` `add_title_language(pool, asset, tag)`, behind a new defaulted method `MediaRepository::add_subtitle_language` (default `Ok(None)`, so the memory repositories need nothing). `sr-media` does not depend on `sr-cinema` and still does not; `sr-db` already depends on both, and the repository trait is how `sr-media` reaches Postgres. Normalisation uses `sr_cinema::rules::language` (the editor's own validator: `pt-br` becomes `pt-BR`); `und`, an empty tag and a tag it refuses add nothing. One `UPDATE ... array_append` over the titles the asset is linked to: `video_titles.asset_id`, an episode's show (`video_episodes.asset_id`), a cut (`video_title_variants.asset_id`), and the title/episode the asset was uploaded for (`video_assets.title_id` / `episode_id`). It never removes, skips a title already listing the language, and stops at the editor's 40 entries. The search index holds no languages (text fields only), so the column is all there is to update; the catalog's `subtitle_language` filter and the facet read it, which `the_catalog_filter_finds_the_title_afterwards` proves.
Call sites: the upload route and the PATCH route (when it sets `language` or `hidden: false`) call `MediaService::add_track_language_to_title(asset, &track)` (a hidden track adds nothing; a failure is logged and returns none) and put the result in the audit entry; the ladder completion calls it from `PgMediaRepo::complete_job` after the commit for every not-hidden embedded track, so a failure can never undo the completion.

## What the takedown test showed before the line (item 3)

The asset's own block (`hls_asset`) already refused the uploaded subtitle's URL (`/cdn/hls/{asset}/u/{track}.vtt`) with 451 before my line (`an_upheld_takedown_refuses_the_uploaded_subtitle_by_either_lock`, first case). The prefix alone did NOT: my route test failed with 200. `routes/cdn/serve.rs` asks the blocklist about the path's own key (`video/{asset}/u/...`) and asset, and only afterwards maps an HLS path to its stored key (`subtitles/{asset}/...`, or `transcode/{job}/{attempt}/...`), which it never asked about. I added one check after the mapping (`serve.rs`, 10 lines), so the new prefix is a real second lock (and so are the `transcode/{job}/` prefixes on the HLS path). Outside my file list, but in `sr-api`, which nobody else touches this wave; flagged for the commander. The sr-moderation test (`uphold_blocks_transcode_and_original_objects_and_restore_lifts_them`, extended) proves `block_asset` writes the prefix and restore lifts it.

## Does the worker report a GPU name, and from where (item 4)

Yes, from `nvidia-smi --query-gpu=name --format=csv,noheader` (`sr-transcoder/src/encoders.rs` `gpu_name`), run through the existing `CommandRunner` (one argument vector, no shell, 3 second limit, 4 KiB of output), only when the NVENC test encode worked and the worker is not held to the CPU. A missing program or a failure is none. Several cards are joined with ", ". ffmpeg prints no device name in `-version/-encoders/-hwaccels/-filters` or in the NVENC test (with `-loglevel error`), so that line source does not exist; `nvidia-smi` ships with the NVIDIA driver (and with the container runtime on the `--gpus all` image), no dependency, no download. **Never run on a real GPU machine** (this PC's worker is not available): the parse is proven against canned output only.

## The index decision (item 4)

Migration `2930_transcode_jobs_worker_done_idx.sql`: `transcode_jobs (worker_id, finished_at) WHERE state IN ('succeeded','failed') AND worker_id IS NOT NULL`. `transcode_jobs` has indexes on `(priority, id)` for queued rows, `lease_expires_at` for running rows, `(asset_id, id)` and `(state, id)`; none starts with `worker_id`, and no code deletes jobs, so "finished by this worker in the last day" would scan a table that only grows, on every Workers-tab load. Reasoned, not measured (no table of realistic size here). The `running` subselect is served by `(state, id)`; the table holds few running rows.

## Tests

Rust (all run by name; `sr_p68_staff` scratch DB for the Postgres ones):
- sr-api `routes::review::tests`: `the_queue_names_its_submitters_and_claimers`, `the_filters_account_is_named_even_with_no_rows`, `a_deleted_submitter_is_absent_not_an_error`, `the_names_cost_one_query` (a counting lookup: one call for six rows), `a_members_own_list_carries_no_users`.
- sr-api `cinema_media::tests`: `subtitle_routes_add_their_language_to_the_title_and_report_it` (the route wiring: upload, language edit, hidden, shown again, delete, a failing add, the audit `language_added`); `cinema::playback_tests`: `an_upheld_takedown_refuses_the_uploaded_subtitle_by_either_lock`.
- sr-db `tests/media_repo.rs` (`--ignored`): `an_uploaded_track_adds_its_language_to_the_title`, `an_episodes_track_adds_to_the_show`, `showing_a_hidden_track_adds_its_language`, `a_language_edit_adds_the_new_one_and_keeps_the_old`, `a_finished_transcode_adds_the_embedded_languages`, `hiding_or_deleting_a_track_removes_nothing`, `und_adds_nothing`, `the_catalog_filter_finds_the_title_afterwards`, `speed_now_sums_running_jobs_with_a_live_lease`, `an_expired_lease_does_not_count`, `the_day_counts_are_per_worker`. These call `MediaService::add_track_language_to_title` where the route would; the route wiring itself is proven by the sr-api test above with a recording repository.
- sr-media: `the_server_cleans_a_workers_gpu_name`, `an_older_worker_without_a_gpu_field_still_registers`. sr-transcoder: `the_gpu_name_comes_from_nvidia_smi_when_nvenc_works`, `no_gpu_name_without_the_program_or_without_a_working_nvenc`, `the_registration_carries_the_gpu_name_unless_held_to_the_cpu`. sr-moderation: the existing takedown test extended with the `subtitles/` prefix.
- Vitest (single file, `admin/__tests__/p68staff.test.tsx`, 7 tests): a named row, a gone and a deleted account, the claimer, the chip (named, with no rows, fallback), the detail; the worker line in its three states. The existing `review.test.tsx` and `p67reviewSubmitter.test.tsx` still pass.

**Which I watched fail:** only `an_upheld_takedown_refuses_the_uploaded_subtitle_by_either_lock` (200 for the prefix alone, which is how I found the CDN gap). I did not run the base for the others (they exercise new code and fields that do not exist on the base; they cannot compile there).

## Shots

`visual-sweep-shots/p68-staff` from `e2e/tests/p68-staff-shots.spec.ts` (local only, `SR_P68_STAFF=1`): queue-named, queue-filtered, queue-filtered-empty, detail, workers; desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark. A fresh stack has no submissions or workers, so the answers are faked with `page.route` (the queue honours the `submitter` parameter) and the flows are clicked through on the fake: press a name, the chip, a reload, clear, a filter that matches nothing, open the detail. Notes on what I saw are in "Shots: what I saw" below.

## Docs updated

`docs/specs/submissions-and-review.md` (the queue), `docs/05-api-and-gateway.md` (the two answers), `docs/specs/cinema-media.md` (4.4 capabilities and the workers list, 11.4a the languages paragraph), `docs/specs/moderation-compliance.md` (the media path sentence), `deploy/TRANSCODER.md` (what the page shows; an older worker shows no GPU name until updated). Nothing else repeated these facts (searched `docs/`, `deploy/`, `README.md`).

## Sentences for files I do not own

- `CHANGELOG.md` (staff tools): "Admin > Review queues now names who submitted and who is reviewing (@username), also in the filter chip. A video's subtitle tracks add their language to the title's subtitle languages (uploads, language edits, tracks shown again, and finished encodes), so the catalog's subtitle filter finds them. Admin > Cinema transcoding shows each worker's GPU, its speed now and its jobs of the last 24 hours. An upheld takedown also blocks a video's uploaded subtitle files, and the CDN now asks the blocklist about the stored file as well as the link's own path."
- `deploy/UPGRADE.md`: "Migration 2930 adds one partial index on `transcode_jobs`; no action." A worker built before this wave keeps working and shows no GPU name until it is updated (`deploy/TRANSCODER.md` says so).
- `deploy/OPERATOR.md`: nothing needed; `nvidia-smi` must be on the worker's `PATH` for the GPU name (the NVIDIA container runtime provides it).
- The commander: run `node scripts/staff-keys.mjs --write` (moves my six keys); the decisions log rows D-692, D-694, D-695, D-705 are as in the plan. D-695's text is true only with the CDN check added here.

## Proposed inventory rows

- SEC-AUD-213 (REV-RV-001 follow-up): "The review queue and detail named submitters and claimers by 19-digit id." Fixed: `users` on the two reviewer answers, `@username` in the rows, the detail, the chip and the held-by lines.
- SEC-AUD-214 (CIN-ADM-001 follow-up): "Subtitle tracks never reached the title's `subtitle_languages`, so the catalog filter missed titles that have them." Fixed at upload, language edit, show, and transcode completion; no backfill.
- SEC-AUD-215 (D-335 follow-up): "A takedown did not block `subtitles/{asset}/`, and the CDN asked the blocklist only about the path's own key on HLS paths, so no prefix of the stored key could ever lock." Fixed both.
- SEC-AUD-216: "The Workers tab showed NVENC yes or no only." Fixed: GPU name, speed now, jobs of the last 24 hours. **ADM-CNT-006 closes** (the GPU name is only ever learned on a machine with `nvidia-smi`; verified against canned output, not a real GPU).
- SEC-AUD-217: dead keys of waves 67 and 68 removed; `settings.notifications.digest` pending.

## Proposed threat-model rows

- Names on a staff answer: `users` on the reviewer answers carries `id`, `username`, `global_name`, `avatar` of submitters and claimers. Staff with the review permission already see the id and can open the account in Admin, so this reveals nothing new; no email or flag is in the object; member routes carry none.
- A GPU name is untrusted text from a worker: any holder of a worker token can send any string. The server drops control and direction-changing characters, trims and caps it at 80 characters where it stores it; the page renders it as text (React escapes it). It is never used in a path, a query or a command.
- The CDN blocklist is asked about the stored key an HLS path maps to (a takedown of a video now holds even if the asset-level row is lost).

## Owner questions (go to Fable)

- Should a transcode that finishes with embedded subtitle tracks add their languages to a title that was edited by hand to a shorter list (we add, never remove; a staff member who removed a language on purpose will see it come back at the next encode of a not-hidden track)? Default chosen: yes, as D-694 says.
- Cuts (`video_title_variants`) and the "uploaded for" link also count as "the title of this asset"; D-694 names titles and episodes. Default chosen: include them.

## Keys that arrive after me (for the next wave)

`settings.notifications.digest` (`en/usrset-7.ts`; still named on this branch by `Notifications.tsx:266`, presence stops naming it). No other builder handed me a key.

## Stack and cleanup

Binaries built in my own target (`p68-staff-target`), copied out to `p68-staff-bin`; ports 8461 / 5461 / 9161; DBs `sr_p68_staff` and `sr_p68_staff_ui`. The cleanup state is at the end of this file.

## Shots: what I saw

All 30 PNGs in `visual-sweep-shots/p68-staff` (5 views x desktop and phone x Editorial, Lounge Light, Lounge Dark) were generated by a passing run (`2 passed`) and I looked at the representative ones: queue named (desktop light, phone dark), filtered (desktop Editorial), filtered with no rows (phone light), the detail (desktop Editorial), workers (desktop dark, phone light). The submitter shows as `@maya.lindqvist` (underlined link button), a gone account keeps its mono number, a deleted one says "Deleted account", the claimer reads "@sam until ...", the chip reads "Submitted by @sam" even with no rows, and the detail shows "@sam" with the id beside it in mono and "@sam is reviewing this until ...". The Workers tab shows a second muted meta line per worker: the GPU name in normal text colour, "2.4x now" and "14 done, 1 failed in 24 h"; the CPU worker reads "no hardware encoder", the offline one "vaapi"; at 375 px the line wraps between its parts, not inside a number, and the page has no horizontal scroll (the spec asserts it). The queue table on the phone scrolls sideways inside its own wrapper, as before my change.

## Cleanup

Scratch databases `sr_p68_staff` and `sr_p68_staff_ui` dropped; `p68-staff-target` and `p68-staff-bin` deleted; my sr-server and Vite stopped by PID (ports 8461, 5461, 9161 free). Scratch scripts and logs (`p68-staff-*.sh|js|log` in `SIDEROOM_wt`) are outside the repo.
