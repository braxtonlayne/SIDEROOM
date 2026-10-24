# 20 — Launch readiness (`RDY`)

Waves 20 to 24, after the inventory. Plan, acceptance criteria, agent split and owner needs: [dev/roadmap-p20.md](../dev/roadmap-p20.md) (D-352). These items are proofs and the gaps the proofs expose, not new sections of the product. The polish wave already running (item-page age buttons, search-index age refresh, Reports count tile, "move a message") is not listed here.

Same conventions as the other files: tick in the PR that completes the item; an item is done when its proof runs (a CI job, a Postgres or browser test, or a recorded run in the handoff), never when it is only written.

## Wave 20 — First real install and the owner's packet

### RDY-OPS — Operations proofs
- [x] **RDY-OPS-001** [OPS][TEST] Production compose smoke in CI: `prod-smoke.yml` builds the image, runs `deploy/install.sh --yes` against `prod.yml` on the runner, waits for `/healthz` through Caddy, migrates, bootstraps, signs in with `entrance-smoke` → `.github/workflows/prod-smoke.yml` _Proven 2026-10-23: prod-smoke.yml run 39962011447 on PR #46 (head 661ce003) green._
- [x] **RDY-OPS-002** [OPS][TEST] Backup, restore, WAL drill, `sr-cli backup`/`restore --force`/`upgrade --dry-run` against the smoke stack; differences written into `deploy/BACKUP.md` · dep: RDY-OPS-001 _Proven 2026-10-23: prod-smoke.yml run 39962011447 on PR #46 (head 661ce003) green._
- [x] **RDY-OPS-003** [OPS] systemd path verified: `systemd-analyze verify` over `deploy/systemd/*` and a scripted static-binary install that answers `/healthz` · dep: RDY-OPS-001 _Proven 2026-10-23: prod-smoke.yml run 39962011447 on PR #46 (head 661ce003) green._
- [x] **RDY-OPS-004** [BE] `sr-cli preflight`: database and migrations, storage round trip incl. multipart, SMTP connect, public URL and headers, TURN allocation, VAPID, disk, clock skew, secret file modes, esplora and voice node when on; table output, non-zero exit on fail; last step of `install.sh` *(sr-preflight unit tests with fakes, Postgres test `tests/pg.rs`, recorded run in the handoff; integration: `install.sh` runs it last and prints the table (`install.test.mjs`), `prod-smoke.sh preflight` checks the rows from the host in `prod-smoke.yml`, and the E2E `live` job asserts the TURN allocation against `dev.yml`'s coturn; those two CI steps have not run yet, see `feat-p20-integration.md`)* _CI part proven in the same run (host-side preflight stage) and the e2e live job's TURN step._
- [x] **RDY-OPS-005** [BE][FE] Preflight in Admin > System and as the Setup Wizard's last step → `POST /admin/system/preflight` (`OPERATOR`, D-355) · dep: RDY-OPS-004 *(sr-preflight, `routes/admin/system.rs`, `PreflightCard.tsx`; tests: `p20preflight.test.tsx`, admin route tests, route matrix)*
- [x] **RDY-OPS-006** [BE][TEST] `sr-cli preflight --alert-test` posts a test card to `#alerts` and the Owner mail through the real `StaffFeed` path · dep: RDY-OPS-004 *(`staff_feed::alert_test`; Postgres test `the_alert_test_posts_a_card_and_mails_the_owner_when_on`; RUNBOOK.md)*

### RDY-OWN — The owner's packet
- [x] **RDY-OWN-001** [DOC] `docs/legal/lawyer-brief.md`: every open legal question (D-287, D-293 to D-295, D-297/D-309, D-311, D-314, D-336 (a) to (h), D-341, D-343, D-347) as a numbered question with the project default, where it is set, what changes if the answer differs; "not legal advice" _(`docs/legal/lawyer-brief.md`: 19 numbered questions, every listed decision once, D-351 noted as raising none; `open-questions.md` now points to it; written, not reviewed by a lawyer)_
- [x] **RDY-OWN-002** [DOC] `docs/dev/owner-checklist.md`: every pending owner or integrator action (accounts, host, GitHub, legal, downloads awaiting a yes), ordered, each saying what it unblocks; replaces the action sections of `open-questions.md` _(`docs/dev/owner-checklist.md`; the VAPID row says nothing needs generating, the server creates the key on first start; `open-questions.md` action sections replaced by a link)_
- [x] **RDY-OWN-003** [DOC][OPS] `deploy/GO-LIVE.md`: provision and harden, DNS, install, preflight green, restore verified, monitoring and alert test, Setup Wizard, lawyer settings, first invites · dep: RDY-OPS-004 _(`deploy/GO-LIVE.md`, linked from INSTALL, README and the owner checklist, known to `scripts/check-ops-files.mjs`; steps 4 and 6 name `sr-cli preflight` and `--alert-test`, which land with RDY-OPS-004 and 006; the list itself has not yet been walked on a real host)_

### RDY-DOC — Documentation truth
- [x] **RDY-DOC-001** [DOC] Stale statements fixed: README status, doc 12 section 5 (`sr-cli backup`/`restore`/`upgrade` exist, WAL is scripted), docs/README; `THIRD_PARTY_NOTICES.md` placeholders listed with their upstream source for the owner's approval _(README Status, doc 12 section 5, docs/README and the "To complete" note in `THIRD_PARTY_NOTICES.md` (the web copy was refreshed); the placeholders themselves are unfilled until the owner approves fetching the upstream texts)_

## Wave 21 — Security proof

### RDY-SEC — Security
- [x] **RDY-SEC-001** [TEST][BE] Per-route authorization proof: `route_matrix.tsv` with no `?`/`other` cells and a generated Postgres test calling every route as ten actors, asserting the promised status class; each deviation a code fix with a test or a TSV correction with a reason → `crates/sr-api/tests/authz_matrix.rs` (moves to `crates/sr-server/tests/` by D-359, R-01) *(proven: `authz_matrix.rs` ran green on Postgres, 16 actors over 959 routes; one handler bug fixed, see `docs/dev/handoff/feat-p21-authz-matrix.md`)*
- [x] **RDY-SEC-002** [TEST][BE] Cross-tenant id sweep: every `{…_id}` route called with another guild's, member's or application's id answers 403/404 and writes nothing · dep: RDY-SEC-001 *(proven in the same run: owner_b xtenant and IDOR passes, member against the private channel, no-write fingerprint per group)*
- [x] **RDY-SEC-003** [BE][TEST] Proof order: `/moderation`, `/admin`, `/developers` and `/applications`, `/oauth2`, `/submissions`, then the rest; the handoff records the order reached · dep: RDY-SEC-001 *(order proven: moderation, admin, developers+applications, oauth2, submissions, rest; recorded in the handoff)*
- [x] **RDY-SEC-004** [DOC] Threat model as built through phase 19: new assets, actors and rows (payments watcher, bot gateway and OAuth2, activities, webhooks execute, Web Push, SDK voice bot, moderation register, DSR exports, suspicion signals); every control names a file and a test; no "open" without an `RDY` id → `docs/15-threat-model.md` *Done: `docs/15-threat-model.md` (27 rows), checked by `scripts/threat-model.test.mjs`.*
- [x] **RDY-SEC-005** [OPS] Secret scan without a download: `scripts/secret-scan.mjs` (key blocks, token shapes, `SR_*` secrets with real values, stray `.env`, credentials in URLs), allow-list, tests, wired into `repo-checks.yml` *Done: `scripts/secret-scan.test.mjs`; clean on the tree.*
- [x] **RDY-SEC-006** [BE][TEST] Abuse limits on the new surfaces (reports, appeals, data requests, counter-notices, DM and friend requests, webhook execute, OAuth2 authorize, interaction callbacks, party joins, expression uploads), each a platform setting with a test and a row in doc 05 and Admin > Limits *Done: `crates/sr-api/tests/abuse_limits.rs`, doc 05 "Abuse limits"; Playwright Chromium suite green on `integration/phase21` (35 passed, 1 skipped; no 429 from the new limits).*
- [x] **RDY-SEC-007** [BE][TEST] Rotation and compromise drills as Postgres tests (`rotate-secret-key`, `verify-access-log` tamper detection, `user reset-2fa`, session revocation, bot-token rotation); `RUNBOOK.md` sections quote the commands the tests ran _Proven 2026-10-23: `crates/sr-cli/tests/drills.rs` (3 Postgres tests through the real `sr-cli`); `deploy/RUNBOOK.md` "Lost or suspect secret key" and "Compromised staff account" quote the commands._
- [x] **RDY-SEC-008** [BE][TEST] Upload hygiene fuzz with generated corpora (nested archives, bombs at and over the limit, polyglots, truncations, forbidden names) over the archive inspector, image re-encoder, sticker and sound validators; the doc 12 limit table matches the code _Proven 2026-10-23: `upload_hygiene.rs` in `sr-library`, `sr-storage` and `sr-packages`, `sound_hygiene.rs` in `sr-chat`; found and fixed the Ogg duration overflow in `soundboard.rs`; the doc 12 section 3 table is tested against the code._
- [x] **RDY-SEC-009** [BE][FE] CSP violation reporting → `POST /internal/csp-report` (unauthenticated, rate limited, capped), counters per directive and top blocked origins on Admin > System; paths only, no personal data _Proven 2026-10-23: `routes/admin/csp_report_tests.rs`, `sr-db/tests/csp_reports.rs` (Postgres), `csp.test.ts`, `p21cspReports.test.tsx`; delivery by a real browser was not observed under automation (handoff, Open 1)._
- [x] **RDY-SEC-010** [BE][TEST] One list of sensitive actions requiring fresh reauth, a test that every listed route enforces it, the list in doc 07; `SR_ADMIN_REQUIRE_REAUTH=0` refused when `public_url` is not localhost _Proven 2026-10-23: `crates/sr-api/src/sensitive.rs` (`SENSITIVE_ACTIONS`), `routes/admin/reauth_list_tests.rs` walks every staff row through the router; doc 07 section 6; `SR_ADMIN_REQUIRE_REAUTH=0` is refused off a development host._

## Wave 22 — Media and voice for real

### RDY-MED — Media pipeline
- [x] **RDY-MED-001** [DB][BE] Member video submissions: `/submissions/{id}/cinema/*` (multipart begin via REV-UP-001 sessions charged by REV-CORE-008, attach asset to draft title/episodes with `sr_media::naming`, edit draft, submit); Postgres tests draft → published · dep: CIN-ING-010, REV-UP-001 *(done p22: `routes/submissions/cinema.rs`, `sr-cinema/src/drafts.rs`, tests `routes/submissions/tests_pg.rs`)*
- [ ] **RDY-MED-002** [FE] Cinema > Submit: files or folder, naming tester, upload dock, draft editor, status and reviewer notes, resume via the IndexedDB ledger; Playwright upload of a generated MP4 in the `live` job · dep: RDY-MED-001 *(p22: screen and Vitest done; the Playwright spec `e2e/tests/cinema-submit.spec.ts` waits for its first CI live run)*
- [ ] **RDY-MED-003** [TEST][OPS] `media.yml`: real ffmpeg on the runner, lavfi source (video, two audio, one subtitle), probe → review proxy → planner → `sr-transcoder` CPU worker over the Worker API; ladder, tracks, WebVTT, sprite asserted; playlists and segments checked with `ffprobe`; `docs/dev/media-pipeline-test.md` *(needs the owner's yes to install ffmpeg on the runner)*
- [ ] **RDY-MED-004** [TEST] Slate, Live TV airing across the slate boundary (D-267, D-285 numbering on real segments), re-transcode and the Patron download object in `media.yml` · dep: RDY-MED-003
- [ ] **RDY-MED-005** [BE][TEST] Transcoder resilience with the real worker: lease expiry and retry on a killed worker, corrupt source fails with logs in Admin > Transcoding, two workers claim without overlap · dep: RDY-MED-003

### RDY-VOX — Voice proofs
- [ ] **RDY-VOX-001** [TEST] Voice through TURN relay: `voice-relay.spec.ts` with `iceTransportPolicy: 'relay'` against the e2e stack's coturn running the rendered prod template with the REST secret; relay proven from `getStats` → `e2e/tests/voice-relay.spec.ts`
- [ ] **RDY-VOX-002** [TEST] Voice soak: `voice-soak.yml` (dispatch), ten Chromium contexts, ten minutes, mute/camera/reconnect churn, E2EE on, no SFU restart, every member hears the tone at the end; count and duration are inputs for the phase 2 gate by hand

### RDY-STO — Storage proofs
- [ ] **RDY-STO-001** [TEST] Chat multipart over real S3 in `storage.yml`: an attachment above the part size through the composer path, abort, resume from the IndexedDB ledger after reload against Garage; confirmed parts equal `ListParts` → `e2e/support/storage-live.mjs`
- [ ] **RDY-STO-002** [TEST] Composer reattach over a throttled network and across a server restart; fixes in `lounge/uploads` with unit tests · dep: RDY-STO-001

## Wave 23 — People readiness

### RDY-A11Y — Accessibility
- [ ] **RDY-A11Y-001** [TEST] Browser axe over signed-in routes with a fixture world (Lounge with thread, Settings, Library series and reader, Cinema title and player, party, Admin, Developers, Support) on three browsers, contrast and landmarks; zero serious or critical → `e2e/tests/a11y.spec.ts`
- [ ] **RDY-A11Y-002** [FE] Roving tabindex on the member list, message list, Library and Cinema grids and the TV guide; a skip link in every section shell; the keyboard map has no follow-ups left
- [ ] **RDY-A11Y-003** [TEST] The eight manual-checklist flows walked keyboard-only by Playwright with focus and live-region assertions; the human reader pass (NVDA, VoiceOver, TalkBack) stays listed as unheard until done
- [ ] **RDY-A11Y-004** [FE] Reduced-motion and high-contrast audit across Lounge, Cinema and Library; a token-walk test; Ladle renders both modes

### RDY-ONB — Onboarding and member docs
- [ ] **RDY-ONB-001** [FE][BE] Platform first run after registration: welcome, appearance, 2FA nudge, PWA install prompt, notification permission only on intent, the home server's onboarding if set; `users.first_run_done`; skippable; Playwright register → first message
- [ ] **RDY-ONB-002** [FE][DOC] `/help` as the member handbook (joining and credits, 2FA and passkeys, PWA on iOS and Android, the Lounge for Discord users and the differences, submitting to Library and Cinema, parties, Live TV, privacy with export, delete, reports, operator country and contact, house notes); content in i18n catalogs
- [ ] **RDY-ONB-003** [FE] "What's new": `CHANGELOG.md` kept per wave, rendered at `/help/whats-new`, one-time badge on the help button when the version changes; the integrator checklist in `AGENTS.md` gains the line

### RDY-I18N — Second-locale readiness
- [ ] **RDY-I18N-001** [FE] Hard-coded strings brought to zero in `routes/entrance`, `lounge/shell` and `settings` (`i18n-audit.mjs`); the remainder counted in the handoff

### RDY-DOC — Operator handbook (continued)
- [ ] **RDY-DOC-002** [DOC][OPS] `deploy/OPERATOR.md`: the one entry mapping INSTALL, GO-LIVE, UPGRADE, BACKUP, RUNBOOK, GARAGE, TRANSCODER, FAQ; daily, weekly, monthly checks; the settings an operator must know; what Admin > System's tiles mean
- [ ] **RDY-DOC-003** [DOC][OPS] A playbook behind every alert: each `alerts.yml` rule names a `RUNBOOK.md` anchor, `check-ops-files.mjs` enforces it, Admin > System red tiles link to the anchors; each playbook walked once against the smoke stack · dep: RDY-OPS-001
- [ ] **RDY-DOC-004** [DOC][OPS] Incident playbooks: takedown received, data request received, compromised staff account, lost owner second factor, disk full, restore from backup, CSAM report (lawyer-dependent steps marked, D-336 (h))
- [ ] **RDY-DOC-005** [DOC] Doc 13 exit gates as a status table: each gate with where it was proven (job, handoff, run) or the `RDY` item that will prove it

## Wave 24 — Scale, parity remainders, developer platform remainders (may follow launch)

### RDY-PERF — Scale proof
- [ ] **RDY-PERF-001** [BE] `sr-cli seed --scale`: 20,000 accounts, 500 guilds, 5 million messages, 2,000 chapters, 300 titles via `COPY` in under 30 minutes on CI hardware; refuses non-local without `--force`; tiny-scale test
- [ ] **RDY-PERF-002** [TEST][OPS] Release-build load test on CI over the seeded dataset: 2,000 sessions, 200 msg/s, 300 REST/s, REST p95 < 100 ms, fan-out p95 < 250 ms, first paint at 60 servers < 1.5 s; numbers and run links in `docs/dev/load-testing.md`; each miss profiled and fixed or documented · dep: RDY-PERF-001
- [ ] **RDY-PERF-003** [BE][JOB] Jobs at scale (search rebuild, retention purges, suspicion scan, trending, backup): timed, batched, no lock held over one second; indexes by migration where `EXPLAIN` demands · dep: RDY-PERF-001
- [ ] **RDY-PERF-004** [BE] Gateway memory at 2,000 sessions and the `READY` size at 100 servers measured; a budget test fails when the per-session figure doubles · dep: RDY-PERF-001

### RDY-PAR — Discord parity remainders
- [ ] **RDY-PAR-001** [BE][FE] Group DM invite link (create, copy in the Create DM picker, join by link, Discord's limits) → `POST /channels/{id}/invites` for group DMs · closes the LNG-HOME-005 remainder
- [ ] **RDY-PAR-002** [FE] Emoji picker category sidebar with server icons, scroll sync and keyboard navigation · closes the LNG-EXP-007 remainder
- [ ] **RDY-PAR-003** [DB][BE][FE] Two-colour profile theme, server tag display choice, markdown in About Me through `sr-markdown` · closes the LNG-USET profile remainders
- [ ] **RDY-PAR-004** [BE][FE] DM spam filter and per-server "allow DMs from members" override · closes the LNG-USET privacy remainder
- [ ] **RDY-PAR-005** [FE] View Server As Role as a Lounge-wide preview mode with the exit banner · closes the LNG-SSET-036 remainder
- [ ] **RDY-PAR-006** [FE] GIF autoplay preference, honouring reduced motion · closes the LNG-USET accessibility remainder

### RDY-DEV — Developer platform remainders
- [ ] **RDY-DEV-001** [FE] TypeScript SDK voice in `packages/bot` with its own transport mirroring `sr_sdk::voice`, E2EE via `sr-e2ee-wasm`; Node test against the real SFU in the `live` job *(a Node WebRTC library is a download; if declined, closed as browser-only and recorded)*
- [ ] **RDY-DEV-002** [BE] `sr-sdk` gateway resume and voice reconnect; `examples/voice_bot.rs` run live in CI for one minute
- [ ] **RDY-DEV-003** [TEST] SDK compatibility next pair (discord.py, twilight) in the weekly job, once the first run is green and useful (D-306) *(downloads, optional)*
- [ ] **RDY-DEV-004** [DOC] Developer docs pass: every route and event `developers/docsContent.ts` names exists in OpenAPI (a test); `docs/dev/sdk-compat.md` updated from the weekly run
