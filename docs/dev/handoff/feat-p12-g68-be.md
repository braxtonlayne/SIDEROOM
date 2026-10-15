# Handoff: feat/p12-g68-be (G-68 to G-84, backend and the web side each needs)

Branch `feat/p12-g68-be`, based on `integration/phase12` (admin-ui, admin-mod-be, devplat-be, feat-ui, web-followups and answers merged). Checklist: [answers-followups-p12.md](../answers-followups-p12.md), section "Phase 12 handoff answers (G-68 onward)". Migrations used: `0810_review_queue_cards.sql`, `0811_app_tester_declines.sql` (the numbers in the checklist, 0796 and 0800 to 0802, are taken on other branches).

## Merge breakage found when compiling the merged Rust

None. `cargo check -p sr-server --tests` and later `cargo check --workspace --tests` compiled the merged branch without a single error or warning, so no merge fix was needed.

## Done

- **G-68 `#review-queue` cards.** `sr-review` `ReviewEvents::card` (default no-op) is called on submit, resubmit, every decision, publish and withdraw; `BusReviewEvents` turns it into `DomainEvent::ReviewCard`. `StaffFeed::on_review_card` (`sr-server/src/staff_feed.rs`) posts the card (`review_card`: title, kind, submitter, item count, Approve, Reject, link) and keeps its message in `review_queue_cards`; later events rewrite the same message to its outcome without buttons (`StaffVoice::rewrite`, embeds and components are not editable through the message service). A resubmission replaces the old card. `POST /admin/card-actions` handles `sr:review-approve:<id>` and `sr:review-reject:<id>` through `press_review_button` (public, so it is testable): Approve claims when needed (a lease held by someone else answers "Another reviewer has this claimed."), calls `ReviewService::approve` under the kind's review permission and writes `review.approve` to the staff log; Reject changes nothing and returns `link: /admin/review?submission=<id>&reject=1`; a decided submission answers "Already decided." (the check for the button on the message is skipped for these ids, so a stale card still answers). `CardActionResult` gained an optional `link`. Web: `parseStaffButton` knows both ids, the card shows the server text and follows `link`; `/admin/review?submission=<id>[&reject=1]` opens the Library tab on that submission, claims it if free and opens the reject dialog. REV-RV-005 ticked (thumbnails and a Request changes button are not on the card).
- **G-69 Edit an announcement.** `PATCH /admin/announcements/{id}` (`title?`, `body?` alias `message`, `ends_at_ms?` alias `ends_at`; `MANAGE_FEATURES`, re-auth, 409 once ended; severity, `critical` and `send_dm` cannot change, no DM is re-sent), audit `announcement.edit` with the diff. New `DomainEvent::PlatformBroadcast` carries `SR_ANNOUNCEMENT_UPDATE` (`{change, announcement}`) on create, edit and end; the banner hook re-reads `/announcements/active` on it. Web: Edit on showing or scheduled rows, Reuse only on ended ones.
- **G-70** `RankCard.level_roles [{level, role_id}]` (roles that no longer exist are dropped; role objects are readable by every member, so "roles the caller can see" is "roles that exist"). Web: the rewards list shows for every member, from the rank card.
- **G-71** `GET /guilds/{id}/modules` gained a sibling map `member_capabilities {module: {can_respond}}` (suggestions and tickets) instead of changing the existing `modules: {id: bool}` shape. Web `useCanRespond` replaces the Manage Server checks.
- **G-72** Suggestions carry `votes {up, down}` (system seed excluded) and `my_vote`. The web stopped fetching messages to count reactions.
- **G-73** `userfeat_pg_tests.rs` `mod http` runs the real ticket and modules routes over Postgres (open, private thread with opener and staff role, second open refused, staff reply, stranger refused, staff close, archive, reopen allowed, list scoping). It found a real bug: staff without thread powers closed a ticket but the thread stayed open. `tickets::close` now falls back to the opener (thread owner) to archive it. Not the gateway `TestWorld` (that is a gateway fixture).
- **G-74** `POST /guilds/{id}/sr/slowmode/apply {preset_id, channel_ids, minutes?}` and `GET /guilds/{id}/sr/slowmode/presets`. The Slowmode schedules module got a `presets` setting (four built-in presets when empty: light 5 s, medium 30 s, strict 2 min, lockdown 10 min) and `SlowmodePlus::apply_preset`; writes go through `scope_module`, so the audit log names the system user and the module (D-187). With `minutes` a `revert:<channel>` state row is written and the module tick puts the old value back, unless someone changed the channel meanwhile or a schedule holds it. Web: "Apply Slowmode Preset" in the channel menu (Manage Channels, text-like channels) and a form on the module card. FTR-SLOW-001 note updated.
- **G-76** `Guild` and `GuildObject` carry `rules_channel_id`, `public_updates_channel_id`, `safety_alerts_channel_id` (null when unset) in `GET /guilds/{id}`, `GUILD_CREATE` and `GUILD_UPDATE`; the `sr/community` route keeps them. Web `usePublicUpdatesChannel` reads the guild store (no extra request).
- **G-77** `POST /share` is answered with `303` to `/share` in the `sr-api` router. The Rust server does not serve the web app (Caddy does), so `Caddyfile.prod` got the same rule. `SharePage` without content shows "Sharing from another app? Open SIDE ROOM once, then share again." (it cannot know it came via POST after the redirect, so the hint shows whenever nothing was shared).
- **G-78** `POST /library/submissions/{id}/cover` (body `{image}` as a data URI like Discord's icon fields; only the submitter, only while the submission is editable, `library.series` only; JPEG or PNG sniffed, 2 MiB, re-encoded by `img::normalize` to at most 600 x 900, stored under `library/<series>/cover-<id>.<ext>`). The web calls it with `renderPdfCover(file)` after the first PDF upload of a new series (best effort). `PdfReader` saves and restores the page index through the existing chapter-progress route.
- **G-81** `FeatureService::client_flags` (also used by `GET /sr/features/flags`) and a `PlatformBroadcast` of `SR_FEATURE_UPDATE {scope: "platform", flags}` when a client-visible platform flag is written (every write path, not only `routes/settings/features.rs`). Web: `ReportsFlagSync` replaces the whole map from the event (`useClientFlags`).
- **G-82** `DeleteAccountDialog` (typed username, password, authenticator or recovery code with two-factor, the D-124 retention text, `staff`, `owner` and `guilds` refusals as sentences). LNG-USET-006 ticked.
- **G-84** Migration 0811 `app_tester_declines`; `respond_tester(false)` writes it, `invite_tester` refuses within 24 hours with `TesterRecentlyDeclined`; hourly `apps.housekeeping` job (id 810) deletes older rows.
- Docs: doc 05 (guild fields, new routes, `SR_FEATURE_UPDATE`, `SR_ANNOUNCEMENT_UPDATE`), developer-platform spec, inventory 07, 09, 10, 12, 18, the checklist.
- OpenAPI, api-client and MSW handlers regenerated; route matrix rows added and reviewed (no `?` left).

## Deviations from the checklist (please check)

- **Error code**: `900121` is already `SrAppSuspended`; the cooldown uses **`900122` `SrTesterRecentlyDeclined` (409)**. Another branch may also have taken 900122; renumber in `sr-core/src/error.rs` if so.
- **G-78 path**: kept the literal `POST /library/submissions/{id}/cover` although the other member routes are `/submissions/{id}/library/*`. The body is JSON (data URI), since `apiFetch` has no raw bodies.
- **G-68 "re-auth rule"**: `POST /admin/review/.../approve` asks for no fresh second factor today, so the card button asks for none either. Say if approve from chat should require `require_reauth` (the web already retries through `withReauth`).
- **G-68 size**: the card shows the item count, not bytes (no per-submission byte size exists).
- **G-71** is a sibling map, not a per-module object inside `modules`, to keep the existing contract.
- **G-69** field names follow the existing announcement shape (`body`, `ends_at_ms`) with `message` and `ends_at` accepted as aliases.
- **G-81** is published from the feature service (all write paths), not only from `routes/settings/features.rs`.

## Not done

- G-75 (on `feat/p12-lounge-gw-be`) and G-85 (next phase), as instructed. G-86 to G-88 were already done.
- G-87 (parallel route chunk import) needs a throttled browser run.

## Tests run

- Rust: `sr-features` (slowmode presets, `client_flags` broadcast), `sr-gateway` (`platform_broadcast_reaches_every_session`), `sr-admin` (announcement edit), `sr-apps` (cooldown), `sr-api` lib (`features_tests` slowmode, userfeat) and `route_matrix`, `sr-api` `api.rs` (`POST /share`).
- Postgres (scratch DB, `--test-threads=1`): `sr-server` `staff_feed_pg_tests` (11, review card posts, approves once, outcome edit, stale press), `userfeat_pg_tests` (7, votes, level roles, ticket flow, capabilities), `sr-db` `apps_repo` (cooldown), `settings_repo` (guild community ids), `review_repo` (cover).
- Rust gates: `cargo fmt --all --check`; `cargo clippy -D warnings --all-targets` on `sr-core sr-events sr-gateway sr-features sr-admin sr-apps sr-review sr-chat sr-db sr-api sr-library sr-userfeat sr-server`; `cargo test` on those crates (sr-api lib 509, route_matrix 3, the rest green); Postgres: all of `sr-server --lib` (79) and all of `sr-db` and `sr-userfeat` with `--include-ignored --test-threads=1`.
- Web: the full vitest suite (154 files passing, `--maxWorkers=2`), `tsc --noEmit`, eslint, prettier on touched files, `vite build` and the bundle budget (lounge 354.9 of 600 KB, admin 288.9, settings 258.7, library 221.4).

## Owner questions

- Should Approve from the `#review-queue` card need a fresh second factor?
- Is `900122` the right number, and should the cooldown be 409 or 403?
- Should the review card show thumbnails and a Request changes button (REV-RV-005 asks for them)?
