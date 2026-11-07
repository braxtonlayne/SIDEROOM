# fix/p54-ops (wave 54: release-build refusals, jobs, resource limits, ops docs)

Audit follow-ups SEC-AUD-028 to SEC-AUD-031 (`audit-p53/SUMMARY.md` MED themes 6, 7, 8 server side and 12) and the
decisions D-565 and D-570. Base: main after the wave 53 merge (`f581b538`). No UI change, so no visual check applies
(no page, dialog or panel was touched; the staff alert card added for failing jobs is server text on an existing card).

## Commits
- `8b7f5294` SEC-AUD-028 (also carries the SEC-AUD-029 loop guard in sr-server lib.rs/app.rs, shared files)
- `ae7f87f3` SEC-AUD-029
- `fc47ecbe` SEC-AUD-030
- `02e081b7` SEC-AUD-031 (with D-565, D-570)

## Done

### SEC-AUD-028: release builds refuse test-only settings (theme 6)
- `crates/sr-server/src/release_guard.rs`, called from `main.rs` `serve()` before anything starts. In a release build
  (`cfg!(debug_assertions)` off) the server stops with one line per key for: an empty `server.secret_key`;
  `unfurl.allow_private_for_tests`; `storage.endpoint = "memory:"`; `activities.origin_mode = same_host_sandboxed`;
  `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS` unless `SR_PAYMENTS_ESPLORA_URL` is a private service (one-label host such as
  `http://electrs:3000`, or a private/loopback/link-local IP); a non-loopback `server.metrics_bind` while the new
  `server.metrics_token` is empty. A debug build only warns, as before.
- **Why `debug_assertions`:** CI's e2e stack (`.github/actions/e2e-stack`), `media.yml`, `storage.yml`,
  `backup-drill.yml` and every Playwright job build with plain `cargo build` (debug) and use `memory:` storage and an
  empty key; the only release builds are `release.yml`, the server image (`server.Dockerfile`), `load-test.yml` and the
  prod-smoke image. `load-test.yml` ran a release build with an empty key: it now passes `SR_SERVER__SECRET_KEY`
  (`openssl rand -hex 32`). The prod-smoke stack sets a key (install.sh), uses Garage and, after the metrics change
  below, a loopback metrics bind.
- **Metrics:** `server.metrics_token` (`SR_SERVER__METRICS_TOKEN`, also `_FILE`): when set, `/internal/metrics` answers
  only `Authorization: Bearer <token>` (constant-time compare, 401 otherwise; `sr_api::internal_router_with_token`).
  `deploy/compose/prod.yml` bound metrics to `0.0.0.0:9100` for Prometheus; that bind would now be refused, so the
  `monitoring` profile's Prometheus runs in sr-server's network namespace (`network_mode: service:sr-server`), scrapes
  `127.0.0.1:9100`, and Grafana's datasource is `http://sr-server:9090`. No new secret is needed, so the upgrade drill
  (which refuses new `CHANGE_ME` keys) is unaffected.
- **Payments:** besides the release check, the opt-in now applies to the Esplora client only; the price sources always
  use the strict SSRF policy (`payments_jobs.rs`).
- **The tunnel test** (`docs/ops/test-deploy-quick-tunnel.md`, D-450) uses `memory:` and `same_host_sandboxed`, so it now
  builds debug binaries (`cargo build -j 2`, `./target/debug/...`).
- **`sr-cli restore --force`:** before anything is dropped, `pg_restore --list` must read the dump and its table of
  contents must create and load `public._sqlx_migrations` (`ops::check_restore_toc`). A damaged, foreign or
  newer-format dump stops with "nothing was changed". (The checksum check against the manifest was already first.)
- Tests: `release_guard::tests` (each key refused by name, the token allows a wide bind, the Esplora rule, a debug build
  still starts), `sr-api/tests/api.rs::metrics_token_is_required_when_set`, `sr-cli ops::tests::a_restore_source_must_be_a_sideroom_dump`,
  `sr-cli/tests/drills.rs::drill_restore_force_keeps_the_schema_when_the_dump_is_unreadable` (Postgres + pg_restore;
  without the fix the schema is dropped before pg_restore fails).

### SEC-AUD-029: jobs that died quietly (theme 7)
- Confirmed: `jobs::fail` dead-letters at `max_attempts` (5 for singletons), and only `schedule_singleton` at the next
  boot re-arms a dead singleton. Root fix in `sr-db/src/jobs.rs`: a recurring singleton (fixed id below
  `SINGLETON_ID_LIMIT` = 2^40; every enqueued job has a Snowflake id far above) is never dead-lettered; it re-queues
  with the existing capped backoff (5 s doubling, at most 1 h) and keeps retrying. This covers retention purge,
  takedown restore, access-log verification and every other singleton, with no handler edits.
- Visibility: `jobs::failing_singletons` (5 or more failed runs in a row, waiting to retry) feeds the health pass
  (`admin_jobs::run_health`): a new staff alert kind `job_failing` ("Background job keeps failing", links to
  Admin > System) names each job, its failed runs and its last error, at most once per the existing repeat window.
  The Admin > System job list already shows `attempts` and `last_error` for the queued row.
- Event loops: `crate::catch_panic::step` runs one event or tick and logs a panic instead of ending the loop. Applied to
  the Discord bridge event consumer and outbox worker, the bridge sync worker and listener, the resume-cards, last-seen
  and interaction-purge ticks, the SFU event loop, the scrapbook cascade consumer, the integration-policy subscriber
  and the search firehose. (Push, staff feed and voice-start listeners already spawn a task per event.)
- Tests: `sr-db/tests/db.rs::a_failing_singleton_keeps_retrying_and_is_reported` (Postgres; fails without the fix at
  attempt 5 with `dead`), `admin_jobs::tests::failing_jobs_are_named_in_the_alert`,
  `catch_panic::tests::a_panicking_event_does_not_end_the_loop`.

### SEC-AUD-030: resource limits, server side (theme 8)
- **Prefix browse (real):** a recursive listing (`delimiter=` empty) collected every key in the bucket before paging.
  `ObjectStoreStorage::list_dir` now starts after the cursor (`list_with_offset`, S3 `start-after`) and reads at most
  `RECURSIVE_SCAN_CAP` (5000) keys per call. Test `browse::tests::a_recursive_listing_reads_a_bounded_window_per_page`.
- **Directory O(listings x 1000) (real):** `GET /directory` read up to 1000 members per listing for the online count,
  then dropped all but 24 cards. The count is now computed only for the returned page. Test
  `sr-api/tests/lounge.rs::the_directory_counts_presence_for_the_returned_page_only` (counts `count_online` calls: 1, was 3).
- **GIF decode budget (real):** a GIF frame was decoded at its descriptor size before the canvas budget was checked.
  The GIF decoder now gets limits: no frame larger than the probed logical screen, and `max_alloc` from the pixel
  budget. Test `img::tests::a_gif_frame_larger_than_its_screen_is_refused`.
- **EXIF upright decode (real):** `strip_metadata` decoded any oriented JPEG/PNG whole to rotate it. Above 50 Mpx it now
  keeps the pixels as they are (metadata still stripped) instead of decoding, and the decoder gets an allocation limit.
  Test `exif::tests::an_image_over_the_budget_is_not_decoded_to_turn_it`.
- **O(n^2) list dedupe (real):** `sr-cinema` and `sr-library` `clean_list` compared every entry with every kept one and
  checked the cap after the loop; now a hash set and the cap stops the loop when passed (`languages` too). Tests
  `rules::tests::a_huge_list_stops_at_the_cap_and_duplicates_still_fold` in both crates.
- **`request_members limit:0` (partly false):** the whole-list request (empty query, limit 0) needs a bot with the
  privileged `GUILD_MEMBERS` intent, which is Discord's contract. The real gap was a name search with a huge `limit`
  (any session): now capped at 100, as on Discord. Test in `members_request_tests.rs::payloads_follow_discords_shape`.
- **Lyrics parser (real):** `parse_lrc` copied the line once per stamp without bound. Output is now capped at 10,000
  lines and 2 MiB of text, and `[offset:-9223372036854775808]` no longer overflows. Test
  `sr-music-wasm tests::lrc_output_is_bounded`. (The web bundle picks this up at the next wasm build.)

### SEC-AUD-031: docs and the release workflow (theme 12, D-565, D-570)
- `/readyz` behind Caddy: the docs said Caddy does not serve it, but the catch-all proxied it. `Caddyfile.prod` now
  answers `/readyz` with 404, the deploy-files test asserts that block (it used to assert only the absence of a proxy
  block), `prod-smoke.sh wait` checks the 404 once the deployed Caddyfile has the block, and UPGRADE.md says so.
- `docs/dev/server.md`: the `voice.e2ee` row no longer documents a toggle; it says the key was removed and `false`
  stops the server. New `server.metrics_token` row; `metrics_bind` and `secret_key` rows mention the release refusal.
- GO-LIVE section 8, OPERATOR (`compliance.operator_contact`), RUNBOOK (notices) and the owner checklist pointed at the
  signed-in-only `/help`; they now name `/house-notes` and the sign-in page.
- `release.yml`: the third-party `softprops/action-gh-release@v2` (mutable tag, `contents: write`) is replaced by the
  runner's own `gh release create` / `gh release upload --clobber`, and permissions are read-only at the top with
  `packages: write` on the images job and `contents: write` on the publish job only. **Deviation:** the task said to pin
  the action by the SHA already used elsewhere in `.github/workflows`; no workflow pins any action by SHA, and looking
  the SHA up would be a network call, so the action was removed instead.
- D-565: the `deaf` comments in `sr-voice/src/core.rs` (`Flags`) and `sr-voice-proto/src/token.rs` now say deafen stops
  audio only (the code: `forward_audio` skips deaf receivers; video and data paths do not look at the flag).
- D-570: `immutable_rules.rs` documents the e2e-only `sr.e2e.noVoiceE2ee` opt-out; `release-dry-run.yml` (the
  workflow that builds the release web bundle; `release.yml` builds none) fails when `VITE_SR_E2E` is set and when the
  bundle contains `sr.e2e.noVoiceE2ee`. (A current production `dist` was checked: the string is folded out.)
- `node scripts/check-doc-links.mjs`: ok.

## Not done / left
- The one-level listing (`list_with_delimiter`) is still unpaged inside object_store (one folder with millions of
  direct children); the recursive case was the reachable one.
- `GET /directory` still builds every approved card (about four small queries each) to sort by member count; the
  1000-member read per listing is gone. A stored member count would remove the rest.
- WebP animation was checked and is not affected (image-webp allocates each frame at canvas size).

## False or partly false findings
- `request_members limit:0` "pages the whole member list": only for a bot with the privileged intent, by design
  (`members_request.rs` refuses the empty query to user sessions and bots without `GUILD_MEMBERS`).
- "Event loops end on one panic" for push, staff feed and the voice-start listener: those already `tokio::spawn` each
  event, so a panic ends only that task.

## Tests run
All with `CARGO_TARGET_DIR=p54-ops-target`, Postgres scratch DB `sr_p54_ops`:
- `cargo fmt --all -- --check`: clean. `cargo clippy -j 4 -p sr-core -p sr-db -p sr-events -p sr-storage -p sr-cinema -p sr-library -p sr-gateway -p sr-music-wasm -p sr-voice -p sr-voice-proto -p sr-api -p sr-cli -p sr-server --all-targets -- -D warnings`: clean.
- `cargo test --lib` for sr-core, sr-events, sr-storage, sr-cinema, sr-library, sr-gateway, sr-music-wasm, sr-voice-proto: all pass; `sr-core --test deploy_files`: pass.
- `sr-api --lib` (830 pass), `--test api` (23), `--test lounge` (32): pass.
- `sr-cli --bins` (33): pass; `--test drills -- --include-ignored restore`: the new drill passes (pg_restore 17).
- `sr-db --test db -- --include-ignored job|singleton`: the existing dead-letter test and the new singleton test pass.
- `sr-server --lib` (release_guard, catch_panic, admin_jobs, test_deploy, staff_feed tests: 20 pass); `--test immutable_rules` (14 pass, 11 ignored DB phases not run).
- `node scripts/check-doc-links.mjs`, `check-ops-files.mjs`, `secret-scan.mjs`: ok.
- Not run: the full sr-server and authz matrix suites, prod-smoke (Docker), the web build (no web code changed).

## Owner questions
1. **Payments private-host opt-in in release builds.** The task said refuse `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS`; D-358 and
   `prod.yml` use it for the self-hosted `http://electrs:3000`. Chosen: allowed only when the Esplora URL is a private
   service (one-label name or private IP), refused otherwise, and never applied to the price sources. Keep?
2. **Prometheus in the server's network namespace** instead of a metrics token in Compose (a token would be a new
   secret the upgrade drill refuses to invent). After `up -d sr-server` the operator also runs `up -d prometheus`
   (UPGRADE.md). Keep, or move to a token later?
3. **Tunnel test on a debug build.** `same_host_sandboxed` and `memory:` now need a debug build; the tunnel guide was
   switched. Acceptable, or should the tunnel test get an explicit release-build override?
4. **Oriented photos above 50 Mpx** are stored without the rotation (metadata still stripped) rather than decoded.
   Acceptable limit?
5. **GIFs whose frames are larger than their logical screen** are now refused (the probe measured the screen). Rare in
   practice; acceptable?
