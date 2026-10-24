# 13 — Roadmap and Phases

Order is fixed by D-154: **Lounge → Library → Cinema**. Each phase ends with an **exit gate**: the criteria that must be true before the next phase starts. Phases map to GitHub milestones. Inventory item prefixes in brackets.

## Exit gate status

Where each gate below was proven, or what is still missing. **Proven** and **Partly** carry a link to a CI job, a handoff or a recorded run; **Not yet** names the `RDY-` item (or the owner action) that closes it. Nothing is marked proven without a link (`node scripts/check-ops-files.mjs` checks the shape of this table). "Recorded" means the run is written down in [the launch-readiness inventory](inventory/20-launch-readiness.md) or a handoff; the CI runs themselves are on GitHub.

| Phase | Exit gate (short) | Status | Where it was proven, or what is missing |
|---|---|---|---|
| 0 Foundation | Fresh install bootstraps; the Owner invites a friend who registers, signs in with 2FA and sees an empty Lounge | Proven | The Playwright spec [exit-gate.spec.ts](../e2e/tests/exit-gate.spec.ts) in [e2e.yml](../.github/workflows/e2e.yml) on a dev stack; the production Compose stack boots, bootstraps and signs in through Caddy in [prod-smoke.yml](../.github/workflows/prod-smoke.yml), recorded in [RDY-OPS-001](inventory/20-launch-readiness.md) (run 39962011447). The same flow on a real host is the first run of [GO-LIVE.md](../deploy/GO-LIVE.md) |
| 1 Lounge: text | The friend group chats daily for two weeks without going back to Discord | Not yet | A human gate: [owner checklist](dev/owner-checklist.md), "Run the two real-use gates with friends". No automated proxy exists |
| 2 Lounge: voice | 10 people, 5 streams, 2 hours, E2EE verified, no SFU crash; push-to-talk in a background tab | Not yet | RDY-VOX-002 (the ten-context soak waits for a dispatch of `voice-soak.yml`); the TURN relay half is [recorded as proven](inventory/20-launch-readiness.md) (RDY-VOX-001). The 300 Mbit/s egress target was missed on the first run, see [the voice backend handoff](dev/handoff/feat-p10-voice-be.md); the two-hour session is a human gate in the [owner checklist](dev/owner-checklist.md) |
| 3 Staff, admin, donations | A BTC testnet or signet payment unlocks Patron and a server level end to end; staff appointment and revocation work; every private-content access is logged | Not yet | No RDY item covers the signet run; the live Esplora check exists but is ignored by default ([payments-live-test.md](dev/payments-live-test.md)) and needs the electrs and node from the [owner checklist](dev/owner-checklist.md). Appointment, revocation and the access log are covered by tests and by the compromised-account drill in [drills.rs](../crates/sr-cli/tests/drills.rs) (RDY-SEC-007) |
| 4 Library | A member submits a 10-chapter ZIP, a reviewer approves 9 and requests changes on 1, readers read RTL on mobile with progress syncing | Not yet | No end-to-end run is recorded and no RDY item covers it; the mobile half needs the real-device pass in the [owner checklist](dev/owner-checklist.md) |
| 5 Cinema | An uploaded episode is transcoded by a rented GPU worker, plays in Safari and Chrome, airs on Live TV on schedule, and five friends watch it in a synced party in voice | Partly | The browser upload, review, real ffmpeg CPU transcode, HLS ladder and tracks, re-transcode, the slate and Live TV airing are recorded as proven on PR #48 in [media.yml](../.github/workflows/media.yml) (RDY-MED-002 to 005, [handoff](dev/handoff/feat-p22-integration.md)). Not yet run: a GPU worker, Safari playback, the five-person party in voice (RDY-VOX-002 and the [owner checklist](dev/owner-checklist.md) real-device pass) |
| 6 Developer platform | No gate written | Not yet | Write the gate first; the weekly [sdk-compat.yml](../.github/workflows/sdk-compat.yml) job and RDY-DEV-001 to 004 are the nearest proof |
| 7 Moderation and compliance | No gate written | Not yet | Steps 1 to 5 are built and tested (D-315); the lawyer's answers decide the open items, see RDY-OWN-001 ([lawyer brief](legal/lawyer-brief.md)). The compromise and key-rotation drills are Postgres tests in [drills.rs](../crates/sr-cli/tests/drills.rs) (RDY-SEC-007) |
| 20 First real install | The production stack boots from the shipped files on an unprepared machine; backup, restore, WAL drill and the systemd path work | Proven | [prod-smoke.yml](../.github/workflows/prod-smoke.yml) run 39962011447 on PR #46 (RDY-OPS-001 to 003), changes listed in [the ops-smoke handoff](dev/handoff/feat-p20-ops-smoke.md). A real host is still the owner's first run |
| 21 Security proof | Every route's authorization is proven for ten actors; cross-tenant ids are refused | Proven | `authz_matrix.rs`, recorded green on Postgres in [the authz-matrix handoff](dev/handoff/feat-p21-authz-matrix.md) (RDY-SEC-001 to 003) |
| 22 Media and voice for real | The media, voice-relay and storage proofs run on CI | Partly | Recorded green on PR #48 in [the integration handoff](dev/handoff/feat-p22-integration.md) for RDY-MED-002 to 005, RDY-VOX-001 and RDY-STO-001 and 002; the voice soak is RDY-VOX-002 |
| 23 People readiness | Accessible by keyboard and screen reader; a playbook behind every alert; one operator handbook | Not yet | The handbook and playbooks are written ([OPERATOR.md](../deploy/OPERATOR.md)) but not yet walked on a running stack (RDY-DOC-003); accessibility and first-run proofs are RDY-A11Y-001 to 004 and RDY-ONB-001 to 003; the screen-reader pass is a human action in the [owner checklist](dev/owner-checklist.md) |
| 24 Scale and remainders | 2,000 sessions at 200 msg/s with REST p95 under 100 ms on a release build | Not yet | RDY-PERF-001 to 004 |

## Phase 0 — Foundation `[FND, ENT, DS]`
- Cargo workspace, pnpm workspace, CI (fmt, clippy, tests, typecheck, Playwright), `cargo-deny`, release pipeline (static binaries + images).
- `sr-core` (snowflakes, errors, permissions), `sr-db` (sqlx, migrations, job queue), `sr-events`, `sr-features` skeleton, config, tracing, metrics.
- `sr-auth`: accounts, sessions, argon2id, email verification, password reset, TOTP, passkeys, Discord OAuth.
- Platform invites, credits ledger, cap enforcement, invite graph tables.
- Entrance UI: landing, invite check, register, login, forgot password, 2FA step, `/invite/:code` logged-out page.
- Design system: tokens, both kits' primitives, Ladle, icon set.
- `sr-cli bootstrap`, Docker Compose dev stack (Postgres, Garage, Caddy).

**Exit gate:** a fresh install can be bootstrapped; the Owner can invite a friend; the friend can register, log in with 2FA, and see an empty Lounge.

## Phase 1 — Lounge: text `[LNG-*]`
Guilds, channels (text, category, announcement, forum, media, threads), messages (markdown via `sr-markdown`, attachments, embeds/unfurls, replies, forwards, reactions, pins, edits, deletes, polls, voice messages upload/playback), DMs and group DMs, friends, presence, typing, read states, notifications (in-app + Web Push), mentions, search (Tantivy), roles and permissions, server settings (all sections incl. Community, Onboarding, AutoMod, Templates, Emoji, Stickers, Webhooks, Audit Log, Bans, Invites), user settings (all sections), profiles, server invites, the Server Directory, built-in "bot replacement" features (doc 11 §1.2), message retention jobs.

**Exit gate:** the friend group can run its daily text chat on SIDE ROOM for two weeks without falling back to Discord.

## Phase 2 — Lounge: voice & video `[VOX-*]`
`sr-voice` SFU (str0m), coturn, voice channels, DM/group calls, webcams, screen share with audio, simulcast, push-to-talk, voice activity, per-user volume, server mute/deafen, move, disconnect, stage channels, soundboard, E2EE (MLS + frame crypto), sender allowance enforcement, egress governor, voice-channel status, AFK channel.

**Exit gate:** a 10-person voice session with 5 streams runs for 2 hours with E2EE verified and no SFU crash; push-to-talk works in a background tab.

## Phase 3 — Staff Server, admin, donations `[ADM-*, DON-*]`
Staff guild mechanics, platform permissions, appointments, admin pages (accounts, Persons, invite graph, features, limits, directory approvals, access log, system status), `sr-payments` (BTC invoices, entitlements, Patron, Server Levels, invite credit purchases), leaderboards (donors), cost calculator.

**Exit gate:** a BTC testnet/signet payment unlocks Patron and a server level end-to-end; staff appointment and revocation work; every private-content access is logged.

## Phase 4 — Library `[LIB-*, REV-*]`
Submission/review pipeline (shared), reading catalog, archive import, page → WebP, naming conventions, metadata editor, reader (all modes), progress sync, bookmarks, ratings, reviews, comments, lists, follows and update notifications, trending (friends/known/global), contributor leaderboards, downloads for Patrons, age gate.

**Exit gate:** a member submits a 10-chapter series as a ZIP, a reviewer approves 9 chapters and requests changes on 1, and readers can read it RTL on mobile with progress syncing to desktop.

## Phase 5 — Cinema `[CIN-*, TV-*, WP-*]`
Video ingest (browser, batch, watched folder, storage import), probe, transcode workers, HLS ladder, player (subtitles, audio tracks, skip intro, next episode, PiP), catalog pages (Discover, Movies, Series, Anime, My list, title detail), ratings, reviews, trending, contributor leaderboards, Patron downloads, **Live TV** (channels, guide import, schedule engine, virtual-linear HLS), **watch parties** (in Cinema and from Lounge channels).

**Exit gate:** an episode uploaded in the browser is transcoded by a rented GPU worker, plays in Safari and Chrome, appears on a Live TV channel at its scheduled time, and 5 friends can watch it in a synced party while in voice.

## Phase 6 — Developer platform `[DEV-*, EXT-*]`
Developer applications, developer portal, bot gateway/intents/interactions/commands, OAuth2, test-server restriction, release review, `sr-sdk` + JS SDK, Discord compatibility suite, activities + Embedded App SDK, theme store, profile layouts ("MySpace mode"), packages and recipes.

## Phase 7 — Moderation & compliance `[MOD-*]`
Inventory: [19-moderation.md](inventory/19-moderation.md). Built (D-315): steps 1 to 3 (cases and reports, the takedown register, the data-request queue) in P18, steps 4 and 5 (the dashboards, suspicion review) and the age tools in P19. Mailing the templated takedown reply (N-35) and mandatory external reporting wait on the lawyer (D-336).

Reports, cases, holds UI, the full ban-reason taxonomy, inviter-suspicion review tooling, takedown workflow, data-subject request tooling, NSFW/age policy enforcement, staff dashboards.

## Waves 20 to 24 — Launch readiness `[RDY-*]`
Inventory: [20-launch-readiness.md](inventory/20-launch-readiness.md). Plan: [dev/roadmap-p20.md](dev/roadmap-p20.md) (D-352). The inventory is ticked, but most exit gates above were never run for real (no Docker, ffmpeg, real host or coturn on the agent machines), so these waves build proofs first: 20, the first real install in CI and the owner's packet; 21, security proof; 22, media and voice for real (and the member video upload path); 23, people readiness (accessibility, first run, the handbooks); 24, scale proof, Discord parity remainders and developer platform remainders, which may follow launch. RDY-DOC-005 turns the exit gates above into a status table.

## Later / ideas
Native desktop (Tauri, Rust) and mobile apps; Lightning and Monero; AV1 rendition; offline downloads in native clients; **bounties** ([spec](specs/future-bounties.md)); multi-node scale-out (`PgNotifyBus`).
