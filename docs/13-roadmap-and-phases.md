# 13 — Roadmap and Phases

Order is fixed by D-154: **Lounge → Library → Cinema**. Each phase ends with an **exit gate**: the criteria that must be true before the next phase starts. Phases map to GitHub milestones. Inventory item prefixes in brackets.

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
Reports, cases, holds UI, the full ban-reason taxonomy, inviter-suspicion review tooling, takedown workflow, data-subject request tooling, NSFW/age policy enforcement, staff dashboards.

## Later / ideas
Native desktop (Tauri, Rust) and mobile apps; Lightning and Monero; AV1 rendition; offline downloads in native clients; **bounties** ([spec](specs/future-bounties.md)); multi-node scale-out (`PgNotifyBus`).
