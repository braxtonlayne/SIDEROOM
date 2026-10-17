# Handoff: feat/p15-k2 (Wave 15 follow-ups K-15 to K-18, K-20 to K-23)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p15-k2`, branch `feat/p15-k2` (base integration/phase15 with `feat/p15-answers`, `feat/p15-devplat` and `feat/p15-browser` merged in). Local commits only, nothing pushed. Source of the work: `docs/dev/answers-followups-p15.md`, section "Wave 15 follow-ups", decisions D-258 to D-263. K-19 is another agent's.

Migrations: **1020** `profile_media` (K-17) and **1021** `identity_moves` (K-20). 1022 to 1029 stay free.

## Done

| Item | What |
|---|---|
| K-15 | `PackageService::decide` refuses the package's own author (403 "You cannot review your own package.", new `PackageError::SelfReview`) for approve, reject and revoke. A submitted version posts a `#review-queue` card; the decision closes it once through `StaffVoice::close_card_at` ("Approved by @x", "Rejected by @x", "Revoked by @x"), a withdrawal and an author delete close it with "Withdrawn". Wiring: `sr_packages::service::PackageEvents` (trait, `NoPackageEvents` default), `sr_api::routes::packages::BusPackageEvents` publishes the new `DomainEvent::PackageCard`, `staff_feed.rs` `on_package_card`. The Review Queues page opens its Packages tab for the card's `?package=` link. Spec `submissions-and-review.md` and inventory EXT-PKG-003 updated. |
| K-16 | `GET /profile-pages/{username}` and `GET /users/{id}/profile-page` fill `gallery.items[].proxy_url` (signed `/cdn/external/...`) for each `url` item (`proxy_gallery` in `routes/packages/handlers.rs`); none for a host the image proxy's SSRF policy refuses up front (deny list, literal addresses, non-http) and none when no image proxy is registered. `blocks.tsx` renders a picture whose `src` is only a `/cdn/external/` path, a placeholder tile with the caption when there is none or the picture fails to load, and the link under it. Songs unchanged. Spec 3.12 and EXT-PRF-003 updated. |
| K-17 | Migration 1020. `POST|GET /users/@me/profile-media`, `DELETE .../{id}` (`routes/userset/profile_media.rs`; the image is the request body, `?kind=background|gallery`), new `MediaKind::ProfileMedia` (banner pipeline, public `/cdn/avatars/{user}/{hash}` namespace, hash scoped per row so two rows never share a file), `MediaDeps::store_raw`. `sr-packages`: `style.background {kind, pattern?, image_id?}` (old `pattern` + `banner_background` still read), gallery items `{image_id}` or `{url}`, `parse_doc_with(.., Some(&MediaIds))` refuses foreign ids, `set_doc` drops unreferenced pictures (rows and files), deleting a picture strips it from the saved document, `ProfilePage.media`. Account deletion (`DeletionReport.profile_media_keys`, files removed) and the data export (`activity.json` `profile_media`). Web: upload and remove in the style panel and the gallery form, renderer draws the picture behind the pattern (`.pp-bgimg` + `.pp-patlayer`). |
| K-18 | `validate.rs`: `slug_reserved` (shipped theme ids from `sr_theme::BUILTIN_THEME_IDS` plus `default`, `sideroom`, `system`, `admin`, `staff`). `DELETE /packages/{id}` by the author (409 "Approved packages can only be revoked by staff", pending versions withdrawn, artifacts deleted). Guestbook full answers "This guestbook is full"; `ProfilePage.guestbook_count`/`guestbook_limit` (owner only) and a line on the owner's page. |
| K-20 | Migration 1021 (`unlinked_at`, `moved_to_account_id`; the unique pair became a partial unique index over live rows; `find_by_discord`, registration matching and the account search read live rows only). `StaffExtrasRepository::move_identity` (Pg and memory), `AdminService::move_identity` (MANAGE_ACCOUNTS, reason required, audit row `person.identity_move`, system DM to the old account), `POST /admin/persons/{id}/identities/{identity}/move` in `routes/admin/people.rs` (there is no `persons.rs`) behind `require_reauth`, email to the old account when its address is verified (`AuthService::send_notice_mail`). `PersonDetail.identities`. Web: identity list and "Move to account" dialog in `PersonDrawer`. |
| K-21 | `sr_e2ee` field on op 4; with `voice.e2ee` on a fresh join without it answers `SR_VOICE_ERROR e2ee_required` and issues no token (`JoinDenied::E2eeRequired`); after `VOICE_SERVER_UPDATE` the member owes op 101 `hello` within `HELLO_TIMEOUT` (10 s), checked by the voice sweep (`e2ee_hello_sweep`), and is removed with the same code. `sr_sdk::voice::E2eeCall::join_state`. Web: `e2eeDeclared()` (flag on and a transform API) adds `sr_e2ee: true` to the join frames; `e2ee_required` shows "This server requires encrypted calls; your browser cannot join." The e2e observer (`sr.e2e.noVoiceE2ee=1`) still declares and sends `hello` (it runs no MLS, so without `hello` it would be dropped after 10 s). Doc 05 op 4 row and code list, spec `lounge-voice-video.md` section 5 items 3 and 6. |
| K-22 | See "K-22 profile" below. |
| K-23 | DEV-BOT-007 note, `crates/sr-sdk/README.md`, spec 3.13 default-on sentence, XC-TEST-001 ticked with the D-261 note, XC-CI-005 note. |

OpenAPI, the TypeScript client and the MSW mocks are regenerated; route matrix rows added (`DELETE /packages/{package_id}` author, the three profile-media routes self, the identity move staff); `openapi_names` `KNOWN_CLASHES` is still empty.

## K-22 profile

A vitest benchmark (`apps/web/src/store/readyManyGuilds.test.ts`) feeds a synthetic READY plus 60 GUILD_CREATE (25 channels, 12 roles, 40 members and presences each = 1500 channels, 2400 members) through the reducer. Top frames of the old path: `addUser` and `putChannel` copied the whole `users` / `channels` map once per member and per channel (quadratic in the state size), and every GUILD_CREATE was its own `set()`, so each screen rendered 61 times.

| | before | after |
|---|---|---|
| reducer, READY + 60 GUILD_CREATE (node, unoptimised) | about 900 ms | about 80 ms |
| store updates for the burst | 61 | 1, then a few small ones for members |

Changes: `addUsers`, `putMembers`, `putChannels` copy once; `applyDispatches` (one update); `connect.ts` collects READY and GUILD_CREATE for 30 ms (`BURST_MS`) and flushes before any other event, so order is kept; members and presences of a burst are indexed 6 servers per timer tick (`PEOPLE_CHUNK`) after the servers, channels and roles are visible; READY keeps the cached message windows marked `stale` (the open channel's `useLiveMessages` / `useMessages` fetch the newest page again, which replaces the stale window, dropping messages deleted meanwhile).

**Not verified:** the acceptance number (first channel paint under 2 s with 60 guilds on the dev build). It needs a real browser, which the brief forbids on this host. The 30 s in `feat-p15-browser.md` was dev-mode React rendering after each of the 61 updates; the burst removes most of those renders, but please confirm with the e2e Owner account (about 60 servers) in Chromium.

## Tests run

- `sr-packages` all (unit + Postgres `--include-ignored`): 18 unit, 9 db.
- `sr-server --lib staff_feed` incl. the new package card test; then `sr-server --lib` (98 passed, `--include-ignored --test-threads=1`), `sr-api` lib (606) and its `api`/`guilds`/`lounge`/`openapi_names` tests, `sr-admin` (46), `sr-auth`, `sr-storage`, `sr-events`, all on a scratch database that is now dropped. The `--include-ignored` run of `route_matrix` was stopped after it ran far longer than the normal run (which passes).
- `sr-db` full `--include-ignored --test-threads=1` (all ok, incl. migration prefixes, account deletion with profile media, data export, `admin_ext_repo` identity move).
- `sr-gateway` 177 passed (4 new E2EE declaration and hello timeout tests).
- `sr-sdk --features voice-e2ee --test voice_e2ee` 5 passed.
- `sr-api --test route_matrix --test openapi_names` passed.
- `cargo fmt --all`, `cargo clippy --all-targets -D warnings` on the touched crates.
- Web: `tsc --noEmit`, `eslint`, `prettier` (changed files), full vitest (`--maxWorkers=2`: 2241 passed; two timing flakes under load that pass alone: `p13entrance` hero upload and the editor a11y test, whose wait was made robust), `pnpm build` and `scripts/check-bundle-budget.mjs` (all budgets ok).
- Not run: Playwright (not allowed here). `e2e/e2ee/voice-e2ee.spec.ts` needs a Chromium re-run (observer declares and says hello); the Firefox/WebKit informational step is as before.

## Decisions I took (please confirm)

1. **Package review cards have one link button, no Approve button.** The Activity bundle card family the brief mentions does not exist on this branch (it is on another one), so I wrote the card on top of `close_card_at`. Approving needs the review page anyway (the diff, the zip); a rejection needs a note. If you want a one-press Approve like Library submissions, it is a small follow-up in `staff_feed.rs`.
2. **No table for package cards**: the card is found again by the `?package=<version>` link in its button (`close_channel_card`). One closing per card follows from `close_card_at` ignoring a card without buttons.
3. **K-16 up-front refusal**: the signed path is withheld only for what the SSRF policy rejects without a lookup (deny list, literal IPs, allow list). Size and time refusals happen when the proxy fetches, so the client's `onError` placeholder covers them.
4. **K-17 upload shape**: image as the raw request body with `?kind=`, not a data URI. A new background replaces the old one instead of answering 409. Saving a document drops unreferenced pictures at once (an uploaded picture that no saved document names is gone at the next save).
5. **K-20 migration 1021** was needed: two rows for one Discord user cannot coexist under the old unique constraint. "Suspended" is the `deactivated` state; a banned Person, a `platform_banned` or `deactivated` source, or a target that is not `active` are refused (409). A deleted source account may be moved from (its identity is otherwise stuck, D-030). A history row can be re-linked only while no live row exists.
6. **K-21**: the declaration is checked on a fresh join only (not on in-place mute/camera updates). The web client declares only when the build flag is on and the transform API exists.

## Still open

- K-22 acceptance measurement in a browser (above).
- E2E re-run of `voice-e2ee.spec.ts` and a CI pass of the `live` job.
- Nothing else skipped.

## Open owner questions

1. Package `#review-queue` cards: add an Approve button (press runs the review under the presser's permission), or keep the link-only card?
2. K-17: should uploading a picture that is not yet in a saved document survive until the next save (grace period), or is "dropped at the next save" right?
