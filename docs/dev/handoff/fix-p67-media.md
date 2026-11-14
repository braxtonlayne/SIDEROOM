# Wave 67, media: handoff (SEC-AUD-186, SEC-AUD-199..202; D-685, D-686, D-687)

Branch `fix/p67-media`. Commits: `SEC-AUD-186, 199, 202` (footer note, channel logo, phone swipe), `SEC-AUD-200, 201` (subtitles server and panel), then the OpenAPI/client regeneration and this handoff.

## Done
- **186 footer note.** `tv.footer_note` (group `tv`, text, 200, READERS row), `PgFooterNote` beside `PgSlateSetting`, `TvService::with_footer_source` (10 s cache like the slate), `Guide.footer_note` filled in `guide`, cleaned (control characters and breaks become spaces, trimmed, cut at 200 characters). `normaliseGuide` reads it. The phone guide shows the same footer (Guide.tsx renders it under both layouts).
- **199 logo.** Before: `/cdn/tv/...` was **not served** (`KINDS` had no `tv`: unknown kind, 404). Now `("tv","tv")` is a CDN kind. `PUT/DELETE /admin/tv/channels/{id}/logo` (`tv.channel.logo.set|clear`), re-encoded through `sr_storage::img` (bounded 512, metadata stripped, alpha kept, animation refused), key `tv/{channel}/logo-{id}.<ext>`, old object deleted (failure logged). Update accepts only the channel's own key or null; create must omit it. A channel delete deletes the object (channels are hard-deleted). Editor row: Upload logo, Remove, size/type checks, "save first" for a new channel.
- **200 subtitles server, 201 panel.** See below. Panel: `SubtitlesPanel.tsx`, opened by a "Subtitles" button per linked video in the asset panel (not tied to a playable flag: the server decides).
- **202 swipe.** The agenda now has a selected channel (the now panel's), moved by a sideways swipe of at least 48 px, more sideways than vertical (touch or pen, never blocks scrolling: `touch-action: pan-y`, nothing prevented), no smooth scroll under reduced motion, a polite status line. There was no phone control that picked a channel before; the channel controls (tune in, favourite, programmes) stay the keyboard path.

## How an uploaded subtitle is stored, and why a re-transcode cannot touch it
Converted by `subtitles::convert_upload` (format from content; SubRip, WebVTT via the new `vtt_to_webvtt`, ASS kept too), stored as `subtitles/{asset}/{track}.vtt` (+ `.ass`) on the instance bucket. The row has `source='uploaded'`, `job_id` NULL (nullable now), paths `u/{track}.vtt`, which the HLS CDN maps (`PlaylistRenderer::storage_key`) to that key only for a track the playback answer lists. A ladder completion deletes only `embedded` rows, under the asset-row lock; uploaded ids stay, ordinals are renumbered after the embedded ones; nothing queues `subtitles/` in `cinema_media_gc` (it lists only replaced ladder prefixes). Retention deletes uploaded rows and files with a removed asset's original.

## Edits of embedded tracks
Survive, by `source_ordinal` (the plan's `-map 0:s:<n>`, new column). Staff edits set `edited`; at a re-transcode edited rows give label, language, flags and default back to the new row of the same stream; hidden is carried always. **Deviation from the contract ("nothing is backfilled"):** the migration reads `source_ordinal` for old rows back from the newest finished ladder job spec; without it edits made to pre-wave rows would be lost at the next re-transcode. A row no plan names resets once. A staff-chosen default wins over the plan's.

## Not done / questions
- Not built: nothing of the five items. The title's `subtitle_languages` is touched by no transcode completion today, so uploads leave it alone (**owner question:** should a completion or an upload add languages to it?).
- BYOS: all assets are on mount 1; an asset on another mount answers an upload 409 ("this video's storage does not take subtitle uploads"); the panel shows the sentence.
- Follow-up outside my files: `crates/sr-moderation/src/takedowns.rs:581` blocks `originals/{asset}/` and `transcode/..` on an upheld notice; add `subtitles/{asset}/` (text only, low risk).
- authz matrix: seeded a real asset row and an uploaded track under their own overlay keys (`subtitle_asset_id`, `subtitle_track_id`) so the other asset routes stay in WRITE_OBSERVED (length unchanged). Not run locally (CI).

## Tests (watched fail without the change: see the end)
Rust: `the_guide_carries_the_operators_footer_note`, `a_footer_note_with_control_characters_is_cleaned`, `the_tv_footer_note_is_plain_text_of_at_most_200_characters`, `the_footer_note_setting_is_read_from_the_settings_row` (ignored, PG), sr-tv `a_logo_is_reencoded_and_bounded`, `an_svg_or_a_renamed_script_is_refused`, `replacing_a_logo_deletes_the_old_object`, `an_update_cannot_point_the_logo_at_another_key`; sr-api `a_logo_url_is_served_to_a_member_and_refused_unsigned`, `staff_without_the_permission_get_403_and_logo_changes_are_logged`, `subtitle_changes_are_validated_logged_and_scoped_to_their_asset`, `an_uploaded_subtitle_is_served_from_its_own_prefix_and_an_unlisted_one_is_not`; sr-media subtitles (VTT normaliser, sniffing, refusals); sr-db `media_repo` (ignored, PG): the ten subtitle tests named in the task. Web vitest: p67MediaLogo, p67MediaSubtitles, guide (footer, phone footer, swipes), model, cinema (Subtitles toggle).

## Docs updated
`docs/specs/cinema-media.md` (8.5 row, 11.1, 11.2, 11.4, new 11.4a), `docs/specs/cinema-live-tv.md`, `docs/05-api-and-gateway.md` (guide, kinds table, both route families), `docs/04-data-model.md`. Searched for "subtitles manager", "logo upload", "footer note", "not on the server yet": the stale lines were the Live TV spec ones, fixed; no `deploy/` mention.

## CDN kinds matched
Only `routes/cdn/path.rs` `KINDS` (parser/key map) and `PUBLIC_KINDS` (tv is signed, not public); `serve.rs` special-cases `hls` only; `admin_jobs.rs` `storage_part` and `downloads.rs` `cdn_path_for_key` are storage-census and download helpers and do not need `tv`.

## For files I do not own
- `deploy/OPERATOR.md`, settings table: `tv.footer_note` | Live TV | text, at most 200 characters, empty | "A line at the foot of the Live TV guide in place of the built-in text. Plain text, no links. Applies within seconds."
- `deploy/UPGRADE.md`: migration 2890 adds columns to `video_subtitle_tracks` (forward-only, fast) and backfills `source_ordinal` from finished ladder jobs.
- Changelog: "Staff can add, edit, hide and delete subtitles per video (they survive re-encodes), upload a Live TV channel logo, set the guide's footer line; the phone guide swipes between channels."
- Dead keys: `admin.channelEditor.logoNotAvailable`, `admin.channelEditor.logoNotAvailableCallSign`. New strings are in `en/p67-media-1.ts` and `-2.ts`.

## Proposed inventory rows
SEC-AUD-186 `tv.footer_note` (closes TV-UI-009); SEC-AUD-199 channel logo upload and `tv` CDN kind (closes TV-ADM-001); SEC-AUD-200 subtitles manager server (D-685); SEC-AUD-201 Subtitles panel (closes CIN-ADM-001); SEC-AUD-202 phone swipe (closes TV-UI-010).

## Proposed threat-model rows
An uploaded subtitle is hostile input (parsed and re-written, never stored as sent, 20 MiB / cue caps, escaping, hidden tracks unserved); an uploaded image (sniffed, re-encoded, 2 MiB, no SVG, no animation); a logo key pointing at another object (refused: own key or null only).

## Verification
- Locally: sr-tv (109), sr-media subtitles (22), sr-api tv, cinema_media, cdn, playback tests (95), route_matrix (6), sr-db `media_repo` `--ignored` (23), sr-server `tv_wiring` `--ignored` (6), clippy `-D warnings` on sr-media, sr-db, sr-tv, sr-api, sr-admin, sr-server (clean), vitest for the touched web files, tsc, eslint, prettier. Not run locally (CI): the authz matrix, the whole-crate suites, the bundle and entrance checks.
- **Watched fail without the change** (run on this branch with the fix lines removed): `a_logo_url_is_served_to_a_member_and_refused_unsigned` (no `tv` kind), `an_uploaded_subtitle_survives_a_retranscode_and_still_plays` and `a_hidden_track_is_in_neither_the_playback_answer_nor_the_playlist` (delete-all replacement, no `NOT hidden`). The others were written with the code, not run against the base.
- **Found by the visual check:** saving a manual or imported-guide channel in the editor answered 422 (the client sent `rules: null`, the server refuses null). Fixed in `tvApi.ts` (no `rules` key unless rule-based); vitest asserts it.
- Shots: `visual-sweep-shots/p67-media` (guide top, footer note of 200 characters, logos in the grid and now panel, the phone agenda before and after a swipe, the editor with and without a logo, uploaded, after a reload, removed, Admin > Settings with and without the saved note, the Subtitles block with four tracks, a refused upload, the delete dialog and after the flow), desktop 1280x800 and phone 375x812, Editorial, Lounge Light and Lounge Dark. The channel editor flow (upload, save, reload, remove) and the Settings flow ran on the real stack; the subtitle flow on a faked track list.

## Cleanup
Scratch databases `sr_p67_media` and `sr_p67_media_ui` dropped; `p67-media-target` deleted; no build or test of mine is running.
