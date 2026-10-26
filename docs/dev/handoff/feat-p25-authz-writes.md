# Handoff: feat/p25-authz-writes (RDY-SEC-011, RDY-SEC-012)

Branch `feat/p25-authz-writes` (worktree `p25-authz-writes`), base `integration/phase25`. Nothing pushed or merged. No migrations used (1340 to 1349 untouched), no new dependency (the `zip` dev-dependency of `sr-server` was already there), `route_matrix.tsv` untouched (it has no reason column; see "Open owner questions").

## Resume here

The proof is built and green. To run it again:

```
SR_TEST_DATABASE_URL=postgres://... DATABASE_URL=postgres://... cargo test -j 2 -p sr-server --test authz_matrix -- --include-ignored --test-threads=1 --nocapture
```

* `SR_AUTHZ_WRITE_ONLY=1` runs only the write phase (about a minute); `SR_AUTHZ_WRITES=0` skips it; `SR_AUTHZ_GROUPS=0,1` picks groups; `SR_AUTHZ_REPORT=<file>` writes one row per call (`pass` = `write`, verdicts `ok`, `shape`, `observed`, `FAIL`).
* Last full run on this machine: 2 minutes 20 seconds wall, 98 seconds of test time when quiet (3 minutes 28 seconds while two other agents built). Inside the five-minute budget of RDY-SEC-001; if CI is slower, split per group with `SR_AUTHZ_GROUPS` (the groups already have a world each).
* Scratch database `sr_p25_authz_writes` is dropped at the end of the run (see the last section).

## What was built

* `crates/sr-server/tests/authz_matrix.rs`
  * **The write phase** (`write_phase`): after the refusal and read phases of each group, every write route of the group is called once as its rightful actor (`rightful_of`) and must be admitted: not 401, 403, 404, and not a 5xx or a dead connection. Order: creates, then edits, then deletes from the leaf to the root (`write_rank`), with named exceptions: `write_ending` (a hold is released after it is reviewed), `write_late` (tier 1: bulk overwrites, bans, the party's removal; tier 2: token and secret resets, the application, webhook, channel and server deletes) and `write_root` (a channel's messages before the channel). A 400 or 422 is admitted and reported with its body (`shape`); `SHAPE_CEILING` (7, 28, 5, 4, 6, 45) only goes down.
  * **`WRITE_OBSERVED`** (41 routes after P25-SEC-1, 42 at handoff, each with its reason) and `WRITE_OBSERVED_LEN`, pinned by `write_observed_is_pinned` (no database). A listed route that is admitted fails the run until the entry is removed: the table can only shrink.
  * **RDY-SEC-012**: the seed enrols `staff_owner` (and the staff offeree) in TOTP; the Owner completes the step-up with a real code once per group (`step_up`), so announcements and the other gated staff writes are asserted. The test config turns `voice` on (embedded SFU on loopback, a UDP range of its own per world) and the seed starts a stage in a stage channel (`stage_channel_id`, plus `stage_new_channel_id` for `POST /stage-instances`). Stickers and sounds are asserted with a generated PNG (`make_png`, real CRCs) and WAV (`wav_uri`). Stickers and sounds take JSON data URIs in this API, not multipart, so no multipart client was needed; `raw_body` / `call_bytes` carry the zip (Activity bundle, packages) and binary (profile picture) uploads instead.
  * Seed additions so the rightful calls reach real resources: a published film with a runtime and a playable asset reference, a published series with a season and an episode, a published edition with a chapter, volumes, comments, lists, reviews, reports, a Live TV block, a watch party (bound to the channel, with a queue and a second participant), packages (approved and pending, submitted as a zip), evasion signals, a staff alert, a data request, an entitlement, a template, a ticket, a reminder, a scheduled message, bookmarks, a profile picture, an Activity bundle, an application emoji, a tester invite, a staff offer, reactions, a pin, a second webhook, a platform invite credit. Per-family placeholders are chosen by `OVERLAYS` (Cinema and Library each have their own `{asset_id}`, `{comment_id}`, `{list_id}`; episodes belong to the series title).
  * Disposable actors for calls that end a session or an account (`disp_pw`, `disp_del`), `offeree`, `victim` and `insider` sessions; `member`, `app_owner`, `disp_*` and `owner_a` have a known test password so the password-confirmed routes are asserted.
  * Weak refusals: the control for a write route is now the write phase's own answer, so `WEAK_CEILING` fell from 0, 28, 12, 0, 8, 79 to 0, 0, 0, 0, 0, 16.
  * Harness fixes the new seed needed: the directory listing is approved only at the start of the write phase (an approved listing lets any stranger join, which would turn the refusal phases into admissions); the fingerprint waits for six steady checks and leaves the system user's direct messages out (the server's own tasks write the offer and tester-invite DMs at times of their choosing).
* `docs/05-api-and-gateway.md` section 5.4 (the write phase), `docs/15-threat-model.md` row T7, `docs/dev/answers-followups-p16.md` R-04 and R-05 ticked with a done note.

## Handler bugs found and fixed (each with its test)

1. **Watch Party heartbeat told strangers the playback.** `POST /watch-parties/{id}/heartbeat` answered 200 with the party's real playback (playing, position, rate, version) to any signed-in account, whether or not it was in the party (an outsider, the owner of another server) once a party is bound to a channel. Fix: `crates/sr-watchparty/src/service.rs::heartbeat` answers a caller who is not a participant with `member: false` and a neutral playback (`PlaybackView::hidden`). Test: `crates/sr-watchparty/tests/db.rs::a_heartbeat_from_a_stranger_reveals_no_playback` (Postgres; the suite passes, 26 tests). The matrix lists the route in `outsider_exception` because a non-member still gets a 200 `member: false` (see the owner question).
2. **Voice status named the kind of a channel before asking who may see it.** `PUT /channels/{id}/voice-status` answered a stranger "Must be a voice channel" (400) for a text channel but 403 for a voice one, an oracle for channel existence and kind. Fix: `crates/sr-gateway/src/voice_status.rs::voice_status_set` asks for the channel permissions first. Test: `voice_ext_tests.rs::who_may_set_a_status_and_what_is_refused` (an outsider on a text channel is `MissingPermissions`).

## Findings that are not handler fixes

* Five refusals are still "masked" in the `rest` group: `DELETE /requests/{id}/claim` answers a stranger 409 "nobody you may release" (a state check before the identity check) and `PATCH /guilds/{id}/soundboard-sounds/{sound_id}` as another tenant's owner (IDOR, full body) answers 400 for an unknown emoji. Neither is an admission or a write; they are reported in the run line (`5 masked`) and left.
* About 95 admitted writes answer 400 (`shape`): they are validation or state errors raised after the call was identified (no 422 from the JSON extractor remains), but they prove the permission check less well than a 2xx. The ceilings keep them from growing; each could be lowered with a better hand-made body.

## Still open (41 `WRITE_OBSERVED` routes, with reasons in the table)

Cinema assets (complete, probe, transcode, original downloads, refresh: the test store cannot start an upload); dead jobs, transcode jobs and workers; stored library pages; alert and card actions (need a posted alert card); appeals (need the mailed token); identity move; slash-command permissions (bearer-only); Activities and their instances; DM calls (call state lives in the voice node); the Cinema Patron download (needs a transcoded asset; the Library one is now called as the seeded Patron `victim`, P25-SEC-1); invoices (no chain source, D-257); interaction callbacks and interaction webhook messages; Discord link, passkeys, external connections; the profile layout (needs an approved `profile_layout` package); integrations; guestbook entries (the victim's page has no guestbook). Two entries are marked "unresolved" in their reason: `DELETE /admin/library/uploads/{upload_id}` (the second seeded admin upload answers Unknown upload) and `POST /watch-parties/{party_id}/leave` (the insider's leave runs after its removal by the member delete).

## Tests run

* `cargo test -j 2 -p sr-server --test authz_matrix -- --include-ignored --test-threads=1`: all six groups proven (1820 + 10933 + 705 + 70 + 590 + 8117 calls, 0 failures; writes asserted 34, 163, 31, 5, 21, 318; admitted 25, 123, 25, 1, 15, 246; malformed 7, 28, 5, 4, 6, 45; observed 2, 12, 1, 0, 0, 27).
* `cargo test -j 2 -p sr-server --test authz_matrix` (no database): `admitted_grammar`, `write_observed_is_pinned` ok.
* `cargo test -j 2 -p sr-watchparty --test db -- --include-ignored --test-threads=1`: 26 passed; `--lib`: 39 passed.
* `cargo test -j 2 -p sr-gateway --lib who_may_set_a_status`: ok.
* `cargo fmt --check` and `cargo clippy -j 2 -p sr-server -p sr-watchparty -p sr-gateway --tests -- -D warnings`: clean.
* Not run: the full `sr-gateway` and `sr-api` suites (only the touched tests), web suites (no web change), OpenAPI (the only schema-visible text, the `member` field comment, is unchanged).

## Decisions taken (conservative defaults)

* The heartbeat of a non-member keeps answering 200 with `member: false` (the web client's `watchparty/controller.ts` reads it to tell a dropped participant "You are no longer in this watch party"); only the playback is hidden. A 404 would leave a dropped participant of a private party looking active.
* `fixup` replaces the sampled `x` strings by `authz_sample` in write bodies only; the refusal phases keep the old sampled bodies, so their results are unchanged.

## Open owner questions

1. *(Decided P25-SEC-1: keep as built.)* **Heartbeat for non-members:** keep 200 `member: false` with a neutral playback (current), or answer 404 to anyone who cannot view the party and teach the client to treat a 404 as "removed"? The 200 still says that a party with that id exists.
2. *(Decided P25-SEC-1: no wallet.)* **A wallet in the test stack?** Seeding an active (test-network) wallet would take the two invoice routes and the Patron downloads out of `WRITE_OBSERVED`; it was skipped as a donations-module decision.

## Scratch resources

Database `sr_p25_authz_writes` (and the `TestDb` clones, dropped by each group) were used; the scratch database is dropped as the last step. No ports were used by this agent beyond the test process's own ephemeral port and the loopback voice UDP range 52000 to 52059.
