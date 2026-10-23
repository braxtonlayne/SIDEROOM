# Handoff: feat/p21-authz-matrix (RDY-SEC-001, 002, 003)

Branch `feat/p21-authz-matrix` (worktree `p21-authz-matrix`), base `integration/phase21`. Nothing pushed or merged. No migrations used (1210 to 1219 untouched).

## Resume here

The proof exists and passes. To run it again:

```
SR_TEST_DATABASE_URL=postgres://... cargo test -j 2 -p sr-api --test authz_matrix -- --include-ignored --test-threads=1 --nocapture
```

`SR_AUTHZ_REPORT=<file>` writes one row per call; `SR_AUTHZ_GROUPS=0,1` runs only those groups. About two minutes with a quiet machine (14.8 thousand calls over 959 routes, six fresh worlds).
The test makes its own database from the template (`TestDb`), so any scratch database URL works. Scratch DB `sr_p21_authz` can be dropped.

## Groups in the order they were proven (RDY-SEC-003)

Each group has its own fresh database and server, so an interrupted run leaves the earlier groups proven. Last full run, all green:

| # | Group | Calls | Failures | Weak refusals |
|---|---|---|---|---|
| 1 | `/moderation/*` | 824 | 0 | 0 |
| 2 | `/admin/*` | 4072 | 0 | 0 |
| 3 | `/developers` and `/applications/*` | 747 | 0 | 15 |
| 4 | `/oauth2/*` | 77 | 0 | 0 |
| 5 | `/submissions/*` | 462 | 0 | 16 |
| 6 | the rest | 8563 | 0 | 108 |

"Weak" means a refusal that was a 404 next to a 404 for the rightful actor (the id names nothing): it proves little. They are listed in the report file. Causes: ids that are not seeded (watch parties, Live TV channels, thread members, DM recipients, nested submission chapters and uploads, application assets, versions, bundles and emojis).

## What was built

* `crates/sr-api/tests/authz_matrix.rs` (new). It boots the real wiring (`sr_server::app::run`, in the test process, ephemeral port, in-memory object storage) against a fresh migrated database, seeds a world and a cast of 16 actors (anonymous; signed in with no server; plain member; member holding the role the private channel allows; server moderator with every permission but Administrator; owners of two servers; platform Owner; Support; basic-bit-only staff; Owner role without an appointment; application owner; a bot installed with three permissions; a bot of another application; two OAuth2 bearers with one scope each), and calls every row of `route_matrix.tsv` over HTTP (raw socket client, no new dependency).
  * Expectations come from the AUTHZ, BOT and BEARER columns (see doc 05 section 5.4). Refusal means 401, 403 or 404; a 400 or 422 would be reported as masked. Last run: 0 masked.
  * Write routes are called with the minimal body and with every property filled in (a no-op PATCH answers 200 to anyone, so the filled body is what reaches the permission check), and with hand-made bodies for the escalation attempts (a role with Administrator, a role handed to someone else, a rename, a vanity code).
  * RDY-SEC-002: `owner_b` calls every route with a tenant id segment using server A's ids, then with its own `{guild_id}` and A's children (IDOR); `member` calls every `{channel_id}` route with the private channel. All refused. After the refused calls of each group a fingerprint of every table (minus the job runner, sessions, staff access log, payment watcher cursor, and the authentication stamps `last_used_at` / `last_seen_at`) must be unchanged: no write.
  * Seeded objects: two servers, roles, a private channel with overwrites, messages, webhooks, invites, a thread, an emoji, a scheduled event, an auto-moderation rule, a poll template, a moderation case routed to server A with a hold, a takedown notice, a submission, an application with bot and command, an OAuth2 grant, the Staff Server with its roles and appointments; every optional server module is switched on in both servers.
* `docs/05-api-and-gateway.md` section 5.4 "How authorization is tested".
* `crates/sr-api/Cargo.toml`: dev-dependency `sr-server` (a dev-dependency cycle; sr-server depends on sr-api). It is how the test gets the real wiring. Cost: `cargo test -p sr-api` now also compiles sr-server and what it depends on. `Cargo.lock` gains one line. This is outside the Split row's file list; the alternative (a test in `crates/sr-server`) was not in the row either.
* `route_matrix.tsv` and `route_matrix.rs` were not changed: the TSV already had no `?` and no `other` cell, and every deviation the run found was a handler bug, not a wrong cell. No TSV correction was needed.

## Authorization bugs found and fixed

1. **The platform Owner was refused package review.** `require_review` in `crates/sr-api/src/routes/packages/handlers.rs` and the `#review-queue` button in `crates/sr-api/src/routes/admin/announce.rs` used `permissions.contains(REVIEW_PACKAGES)`; the Owner holds only the single `OWNER` bit, which implies every permission through `has()`. `GET /admin/developers/packages` and `.../versions/{id}/download` answered 403 "Missing platform permission REVIEW_PACKAGES" to the Owner. Both sites now use `has()`. Test: `authz_matrix` (`staff_owner` is expected to pass every `staff` read; it failed on those two routes before the fix, passes after). The other `contains` sites on platform permissions (`sr-admin/src/rules.rs`) are about grantable bits and are right as they are.

No other authorization hole was found: no refused call wrote, no cross-tenant id reached a resource, no actor outside its row was admitted. Things the run looked at and judged correct (so nobody re-investigates):

* `GET /channels/{id}/activity` and `GET /guilds/{id}/stage-instances` answer 200 with an empty body to a non-member (filtering, no lookup); accepted as "empty answer" (`soft_empty`). Voice is off in the test config, so stages cannot be created; the stage list's filtering is therefore only proven for the empty case.
* `GET /sr/emojis/{emoji_id}` returns name and animated flag to anyone signed in and names no server (`outsider_exception`).
* `GET /applications/{id}/assets` is open to any signed-in user (AUTHZ `user`, asset keys are public).
* `GET /guilds/{id}/sr/applications/form` is open to any signed-in user only while applications are on; closed, only managers see it (403, no existence leak). The closed case was proven, the open case was not.
* Unknown auto-moderation rule answers 400 `General error` (code 0) rather than 404 10xxx; it is the same answer for a rule of another server, so nothing leaks. Left alone.
* No-op edits (`PATCH` with an unchanged value) answer 200 to a member without a permission check (message edit, emoji rename to the same name). They change nothing. The harness runs the rightful actors last in each class so the others' requests are not turned into no-ops.
* Joining by invite code or sending a join request admits an outsider into server A during the observation phase of the run; this is the intended behaviour and does not affect the refusal phase, which runs first.

## Not done / still open (with reasons)

* Per-section platform bits are not asserted. `support` and `staff_basic` are called on every staff row and the results are in the report, but which bit each route needs is not in the TSV, so those cells are *observed*, not asserted. What the report shows: `staff_basic` (STAFF_BASIC only) got no 2xx on any `/admin` write; it passes the permission gate on no route, only validation answers (400/422 for an empty reason) come before it on some. `support` (accounts and invites bits) acts on the account and invite routes only. A column with the required bit per route would let this be asserted; that needs the owner to decide the vocabulary and touches `route_matrix.rs` parse.
* The allow side of write routes is not asserted (a destructive control would break the world for the next routes); only reads are asserted for the rightful actor. Write controls are recorded.
* Admin step-up (`SR_ADMIN_REQUIRE_REAUTH` is left on): routes behind it answer 403 `SrReauthRequired` to the Owner; that counts as admitted. The test cannot switch the variable off (the workspace forbids `unsafe`).
* Not seeded, so only weakly proven: watch parties, Live TV channels and blocks, thread members, DM recipients and message requests, nested submission chapters and uploads, application assets, versions and bundles, library and cinema content ids, takedown items, evasion signals, appeals, reports, suggestions, tickets and RSS feeds (their modules need a configured channel), announcements (need two-factor step-up), stickers and sounds (multipart).
* The Staff Server's `#alerts` channel and other background wiring are not part of the seed. The job runner runs during the test (it is the real server); the payment watcher cursor is excluded from the no-write fingerprint for that reason.

## Tests run

* `cargo fmt --check -p sr-api`: clean.
* `cargo clippy -j 2 -p sr-api --all-targets -- -D warnings`: clean.
* `cargo test -j 2 -p sr-api --test authz_matrix -- --include-ignored --test-threads=1` on Postgres: pass, 105 to 160 seconds depending on machine load.
* Full `cargo test -j 2 -p sr-api -- --include-ignored --test-threads=1` on Postgres, including `route_matrix` and `openapi_names`: pass: lib 656 tests, api 18, authz_matrix 1 (124 s), guilds 23, lounge 31, openapi_names 2, route_matrix 5, doc tests 16; exit 0.

OpenAPI, the api-client and the MSW mocks were not regenerated: no handler changed its API (only a permission check inside two handlers).

## Decisions and questions for the owner

* Is the dev-dependency `sr-api` -> `sr-server` acceptable? The alternative is moving `authz_matrix.rs` to `crates/sr-server/tests/` (the real wiring is there). Either way the integrator should run it once on the merged tree.
* Should the TSV get a column naming the platform bit each `staff` row needs, so the "staff with and without the platform bit" cells can be asserted rather than observed?
* Should an unknown auto-moderation rule be a 404 (10xxx) instead of `General error` 400? Cosmetic, not an authorization matter.

Answered 2026-10-23 (delegated): D-359 (move the test to `crates/sr-server/tests/`, drop the dev-dependency), D-360 (a `PLATFORM` column, asserted; the cheap ids seeded now and a weak-refusal ceiling; the write-side allow phase, stages, multipart and step-up surfaces later), D-361 (unknown rule and template answer 404 `SrNotFound`). Answers P21-1 to P21-3 in `docs/open-question-answers.md`; code follow-ups R-01 to R-06 under "Phase 21 round" in `docs/dev/answers-followups-p16.md`.
