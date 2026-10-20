# Handoff: feat/p17-server-a (M-06, M-07, M-11, M-12, M-15)

## Done
- **M-15** `integration_types` normalised (`[0]` when it contains 0), `[1]` stored and accepted with 200; such commands are never listed, autocompleted, in the directory or dispatched (`Command::guild_offered`). Spec compatibility row and porting-guide page updated.
- **M-11** Approve button `sr:package-approve:<version_id>` on package `#review-queue` cards; `press_package_button` in `routes/admin/announce.rs` (permission, self-review toast, "Already decided.", access log). Card closes through the existing outcome event.
- **M-12** Job `profile.media.sweep` (singleton id 662, daily): old (>24 h) unreferenced `profile_media` rows and files; shares `drop_unreferenced_media` (optional age filter).
- **M-07** Integrations policy applies live. `sr_net::Live<T>` (RwLock<Arc<T>>, no new dependency, deviates from the suggested ArcSwap because of the no-new-deps rule). Read per request by: SSRF policy host lists (`Policy::live_hosts`, so unfurl, image proxy and feeds), unfurler per-host budget, GIF region, lookup enabled list. `integration_policy::LivePolicy` + a subscriber to `FeatureSettingsChanged` for `platform.integrations`. "Next start" help text removed. The unfurl policy check now precedes the preview cache.
- **M-06 (partly)**: `platform.compliance` module (all six settings plus strike window), public `GET /sr/compliance/public`, registration requires and records the affirmed age (`confirmed_min_age`, migration 1070 `accounts.age_declared`), landing/form copy, operator country+contact on the privacy summary and `/help`; docs 06, 09, moderation-compliance section 14.

## Still open
- **M-06 remainder** (left unticked): data-request `due_at` + `POST /admin/data-requests/{id}/extend`, takedown strike counter + review case, counter-notice `restore_after`: the Phase 7 moderation backend (data requests, takedowns) does not exist in the code, so there is nothing to wire. No Setup Wizard exists to ask for country/contact; they are editable under Admin > Features.

## Tests run
cargo fmt --check, clippy -D warnings (sr-api, sr-server, sr-db, sr-auth, sr-features, sr-apps, sr-packages, sr-net, sr-unfurl, all targets); unit tests sr-net, sr-unfurl, sr-features, sr-apps, sr-auth, sr-api lib (619), route_matrix, openapi_names, sr-server integration_policy/lookup; Postgres (scratch DB): sr-packages `db` (11, incl. sweep), sr-api `the_review_queue_approve_button...`, sr-server `staff_feed_pg_tests` (8), sr-db `auth_repo` declared-age test. Web: typecheck, lint, prettier, full vitest (2389), build + bundle budget.

## Decisions / notes
- The age confirmation shares the existing house-notes checkbox (label gains "I am at least {age} years old.") so the e2e specs that tick `/house notes/` keep working; e2e API fixtures send `confirmed_min_age: 13`. The server requires it (absent or below minimum: 400 `errors.age`). Old clients must update.
- Entrance catalog keys went into the core `entrance.ts` fragment (a firstpaint test requires entrance keys there), not a p17 fragment.
- Public endpoint instead of `/sr/features/flags` for the client-visible values (pre-login pages; flags are booleans for signed-in users).
- Compliance country/email are shape-checked on read (the settings schema has no custom validators); bad values fall back to unset/defaults.

## Owner questions
- None new. D-293..D-295 still carry the lawyer-review note.
