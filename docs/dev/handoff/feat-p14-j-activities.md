# Handoff: feat/p14-j-activities (Activity bundles, J-12 to J-17)

Branch `feat/p14-j-activities`, based on `integration/phase14`. Migration `0965`. Local commits only (no push, no PR). Spec: [developer-platform.md](../../specs/developer-platform.md) section 3.10 (rewritten). Checklist: [answers-followups-p14.md](../answers-followups-p14.md) J-12 to J-17 (ticked, with notes). Decisions D-246 to D-248 are implemented as written; deviations and conservative defaults are listed below.

## Done

| ID | What | Where |
|---|---|---|
| J-12 | Activity bundles | Migration `0965_activity_bundles.sql` (table `application_activity_bundles`; `application_activities` gains `reviewed_bundle_id`, `dev_entry_url`, loses `entry_url`, `pinned_sha256`, `reviewed_sha256`; `activity_instances.channel_type`). `sr-activities/src/bundle.rs` (zip validation and unpacking), `service.rs` (upload, submit, decide, supersede, gc, lookup, release condition, shelf), `sr-api/src/routes/activities/{mod,handlers,proxy}.rs`, `sr-server/src/activities_jobs.rs` (`activities.gc`, hourly, job id 965). Web: portal Activities tab (drag a zip, progress, file listing, review history, submit, development URL), admin queue card (hash, size, files, download link). |
| J-13 | Implicit redirect URI | `AppsService::validate_authorize_with`; `routes/developer/oauth.rs` adds `<activity origin>/` for an application that has an Activity while `[activities] enabled`. OAuth2 tab lists it read-only ("Activity (automatic)"); the copy button and the "register this URI" sentence are gone. |
| J-14 | `setActivity` and voice events | SDK `commands.setActivity`, `subscribe(event, listener, {channel_id})` with `VOICE_STATE_UPDATE`, `SPEAKING_START`, `SPEAKING_STOP`; host `activities/host.ts`; presence through gateway op 3 (`activities/presence.ts`, shared with the idle reporter so an idle flip does not clear the Activity). |
| J-15 | Text-channel launch, picture-in-picture | `launch` accepts guild text and announcement channels (no `CONNECT`); instance has `channel_type` and `development`. Composer "+" menu "Start an Activity". `ActivityPanel` (bar and slot, in the channel view) plus `ActivityDock` (Lounge shell) render one iframe, full over the slot or docked and draggable with Return and Leave; the element is never remounted. |
| J-16 | Participants follow the call | `sr-activities/src/voice.rs` `VoiceFollower` on the bus firehose, 5 s debounce, `ActivityService::voice_left`; the last removal ends the instance and dispatches `instance: null`. Spawned only when Activities are enabled. |
| J-17 | Non-GET through mappings | `proxy.rs` `forward_write`; `sr-net` `SafeClient::forward` (`ForwardMethod`, `ForwardRequest`). |

Also regenerated: `packages/api-client/openapi.json`, `src/generated/schema.d.ts`, `src/mocks/handlers.generated.ts`; route-matrix rows (3 new, no `?` left); `openapi_names` KNOWN_CLASHES still empty. Docs: spec 3.10, doc 11 (layer table, section 2 step 4, section 2.2, section 3), doc 12 (Activities install paragraph), doc 15 (T18), doc 05 (instance event wording), inventory `DEV-ACT-001` to `004` ticked and `DEV-PORT-003` wording, i18n fragments `p14-j-activities-1.ts` (new) and edits to `p14-devplat-1/2.ts` (obsolete entry-URL strings removed).

### The security boundary (J-12), in short

* Upload (`bundle.rs`, 10 unit tests): 32 MiB unpacked counted while inflating (headers are not trusted), 2,000 entries, names limited to `A-Za-z0-9 . _ - ~ @ +` and `/`; refused: absolute paths, drive letters, backslashes, `.`/`..`/empty/hidden segments, percent-encoding, control characters, over-long names, symbolic links, special files, encrypted entries, duplicates that differ only by case, a file that is also a directory, files outside the extension allow-list, a missing root `index.html`, zips that do not start at byte 0. Each refusal names the entry.
* Serve (`proxy.rs`, 11 route tests): request paths go through the same rule (`bundle::request_to_path`), objects are keyed `activities/<application>/<sha256>/<path>` so another application's or an unreviewed bundle's files are unreachable by construction; only the approved bundle of a released, non-suspended application is served; content type comes from the extension; CSP, nosniff, no referrer and Permissions-Policy on every response including errors.
* Writes (J-17): mapped prefix only, 1 MiB, the `Origin` header must be the Activity's own, `Content-Type` and body only, redirects not followed; the upstream status is relayed for 2xx and 4xx, anything else is 502.

## Decisions and conservative defaults (confirm or overrule)

1. **Upload route takes the zip as the body, not a staged upload id.** J-12 says "takes a staged upload id of a zip (the existing staged-upload path)". That path (`POST /channels/{id}/attachments`) is bound to a channel and a message flow, which an application owner does not have, so `POST /applications/{id}/activity/bundles` takes `application/zip` directly (limit 32 MiB + 64 KiB). The web tab uploads with `XMLHttpRequest` for progress. If the owner prefers staged slots, the service call `upload_bundle(app, user, bytes)` stays the same.
2. **A reviewed bundle keeps being served while a newer one is pending.** D-246 says any change withdraws the approval; the J-12 text lets an approved and a pending bundle coexist ("neither approved nor pending is superseded"). Implemented: uploading or submitting a new bundle does not touch the reviewed one until staff decide; a change to the **URL mappings** (part of what was reviewed) withdraws the approval at once (the reviewed bundle becomes superseded and the Activity is paused). Changing the development URL does not.
3. **Served only when the application is `released`.** The release condition uses `applications.release_state` (`released`, not `suspended`). An `in_review` application with an older release is therefore paused. If the owner wants "latest released version" read from the version pipeline instead, only `ActivityService::lookup/shelf/may_start` change.
4. **Development builds** are listed and startable only for the owner and accepted testers, in guilds of at most 25 members (counted in `guild_members`); the live URL has no size or content check and is never reviewed. A `development` application with only an uploaded bundle (no `dev_entry_url`) serves nothing.
5. **Same hash twice.** Uploading a zip identical to a non-superseded bundle returns that bundle (so a rejected bundle cannot be re-submitted unchanged; upload something different).
6. **Voice follow debounce is 5 s** (`DEFAULT_DEBOUNCE`). The gateway already waits its own session grace before it publishes `VoiceLeft`, so this only covers moves and quick rejoins.
7. **Upstream statuses of mapped writes:** 2xx and 4xx are relayed, 3xx and 5xx become 502 (a redirect is never followed).
8. **Rate limit of `setActivity`:** 5 per 20 s per frame (`RATE_LIMITED`), because each call reaches the gateway and the person's friends.
9. **Asset names:** `assets.large_image` / `small_image` are resolved through `GET /applications/{id}/assets` (any session may read it, D-237) to the asset id; an unknown name is dropped.
10. **Window layout:** the dock lives in the Lounge shell, so leaving the Lounge for another top-level route (Library, Cinema, developer portal) unmounts the Activity. Pure-Lounge navigation (other channels, DMs, home) keeps it.

## Still open

* DEV-SDK-003, DEV-BOT-007, DEV-APP-007, DEV-PORT-004, EXT-*: not part of this item set (unchanged).
* J-21 is shared with other branches; this branch did its Activities rows only (doc 11, doc 12, doc 15, spec 3.10) and left the box unticked.
* Not built: SDK `getUser`/relationship commands; scripts a bundle loads are now covered (they are in the hashed bundle), but a mapped API's behaviour is not reviewable; SVG files in a bundle are served with the Activity CSP but not sanitised (noted in T18).
* The real browser iframe was not exercised (ports are shared; no Playwright run), nor a real S3 backend (in-memory object storage in tests; `ObjectStorage` is the same trait the other uploads use), nor the real `SafeClient` against the network (a fake transport covers the pinned and refused cases).

## Tests run

* `sr-activities`: 15 unit tests (bundle validation and path rules, URL and mapping rules) and 6 Postgres tests (`tests/db.rs`: bundle states and supersede, gc, release condition, development builds and testers, channel rules, text channels, voice follow with an in-process bus), scratch database, `--include-ignored --test-threads=1`.
* `sr-net` (19, incl. the forward test), `sr-apps` (62, incl. the implicit redirect test), `sr-api` whole package (589 lib tests incl. 11 Activities route tests, route matrix 5, `openapi_names` 2, other targets), `sr-server` lib (48, 49 need a database and were not run).
* `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on `sr-activities sr-net sr-apps sr-api sr-server`.
* Web: `tsc`, `eslint`, `prettier`, vitest for `src/lounge src/voice src/admin src/developers src/activities src/i18n src/store` (125 files, 1,221 tests; Activities: host 14, panel and dock 11, portal tab 8); `vite build` and the bundle budget (lounge 397.9 of 600 KB gzip, developers 224.4 of 400); SDK package tests (8), typecheck, lint.

## Owner questions

See decisions 1 to 3 above; 1 (staged upload id vs direct body) and 2 (coexisting approved and pending bundle) are the ones worth an explicit answer.
