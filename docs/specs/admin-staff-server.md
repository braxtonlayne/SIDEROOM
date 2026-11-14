# Spec — Admin & Staff Server

Phase 3 · Decisions: D-125, D-130–D-133, D-012, D-023, D-031, D-651–D-653, D-658 · Docs 07, 06, 08.

## 1. Two surfaces
1. **The Staff Server**: a normal-looking Lounge server (with a special frame in the rail) for staff conversation, queues, and alerts. Its bot-fed channels carry **action buttons** (Approve, Request changes, Reject, Acknowledge, Open in Admin) so most daily work never leaves chat.
2. **Admin pages** (`/admin/*`): full-screen, using the Server Settings layout and Lounge theme, reachable from the Staff Server header dropdown ("Platform Admin") and the user menu (the entry shows for an account whose own READY says `sr_platform_staff`, D-673; nothing is requested to decide it). Each page requires a specific platform permission. Pages a staff member lacks are hidden. The page list is `ADMIN_SECTIONS` in `apps/web/src/admin/AdminApp.tsx`; the API enforces the same rules, so a hidden page is a courtesy, never the security boundary. The Admin app's code is served only to appointed staff (`/admin` asks `GET /admin/me` first, then imports a chunk from `/app/staff/`; doc 07 section 4, D-651 to D-653). The page labels are the one exception: they still travel in the message catalog every member loads (D-658).

## 2. Admin navigation

| Group | Page | Permission | Contents |
|---|---|---|---|
| Overview | **Dashboard** | STAFF_BASIC | A read-only snapshot, counts live: accounts vs cap (gauge), online now, signed-in sessions, voice rooms, voice egress, storage used, jobs queued, running and dead, open invites, platform-banned and staff counts, review queue sizes (Library submissions, App requests, Cinema reports; each tile only for the holder of that review bit), donations this month vs hosting cost, recent alerts with Acknowledge. For `OPERATOR` and `MANAGE_FEATURES` it also carries the first-run **Setup checklist** (doc 12 §2) |
| | **Crash reports** | STAFF_BASIC | Crashes the app caught in members' browsers, grouped by fingerprint; allow-listed cards only (OPS-CRASH-001, D-430) |
| | **Bug reports** | STAFF_BASIC | Beta bug reports with their debug information and, if sent, the screenshot; listed only while the platform switch `platform.beta_reports` is on (D-647 to D-650) |
| People | **Accounts** | MANAGE_ACCOUNTS | Search (username, display name, email fragment, account id, Discord id, IP), filter by state, detail drawer (profile, sessions, IP history, invites issued, invited by, servers, credits ledger, entitlements, badges, flags, the staff-only archive of a deleted account's original identifiers). Actions: suspend, reactivate, force logout, force password reset, reset 2FA, rename, change email, grant or revoke badges, unlink from a Person, platform ban. The Person drawer (account chain, link an account, move an identity between accounts of the chain (D-262), notes, suspicion level and its history, clear suspicion) opens from here |
| | **Platform bans** | PLATFORM_BAN | Ban with reason category (serious / standard) + note, unban, list with filters and figures. Serious bans trigger inviter suspicion (D-031) |
| | **Moderation** | MANAGE_CASES, MANAGE_TAKEDOWNS, MANAGE_ACCOUNTS, REVIEW_READING, REVIEW_MEDIA or DELETE_CONTENT (any) | Reports, cases, holds, appeals, evasion signals, takedowns, the graduated moderation ladder and subtree cases (docs 05 §5.3d, 07) |
| | **Data requests** | MANAGE_TAKEDOWNS | The data-subject request queue: export and erasure requests, verification, restriction, run, decline |
| | **Storage** | MANAGE_TAKEDOWNS | Every connected bucket, its owner and status; delist and relist a whole bucket (the file is never touched) |
| | **Invites** | MANAGE_INVITES | Every platform invite and its issuer, open codes, revocation, inactive subtrees, the creation freeze and Lock the Doors (a reason is required; locking itself is `MANAGE_FEATURES`, doc 06 §5.3a) |
| | **Invite graph** | MANAGE_INVITES | Collapsible tree, flat list and a 2D canvas view of everyone a root account invited (force-directed layout computed by `sr-graph-wasm`; no WebGL, P10-ADM-8), path-to-root, banned and suspicion markers, open codes, credit grants and revokes |
| | **Servers** | MANAGE_GUILDS_GLOBAL | Search all servers, view metadata, transfer ownership, delete abandoned servers (reason required, audited) |
| | **Staff & roles** | APPOINT_STAFF | Staff list, offers, appoint (account picker + roles), "Appoint directly (emergency)", change roles, revoke, former staff, delegation of appointment rights (Owner only) |
| Content | **Review queues** | per tab | Tabs: Cinema (reports, `DELETE_CONTENT`), Submissions (`REVIEW_READING` or `REVIEW_MEDIA`; shown while the Study ingest-queue switch is on), App queue and All applications (`REVIEW_APPS`), Packages (`REVIEW_PACKAGES`), Directory (`MANAGE_DIRECTORY`: approve or deny listings, categories, featured servers). See the submissions spec |
| | **Jobs** | MANAGE_TRANSCODE | Background work (transcoding, ingest, cleanup): retry a dead job or delete it |
| | **Ingest sources** | PUBLISH_READING | Watched folders, storage imports, naming-convention tester; shown while the Study ingest-queue switch is on |
| | **Library catalog** | PUBLISH_READING | Series/edition/chapter editor, page reorder, re-convert, merge, credits, featured lists and reading lists |
| Cinema | **Cinema catalog** | PUBLISH_MEDIA or DELETE_CONTENT | Title/season/episode editor, assets, renditions, re-transcode, subtitles, artwork, markers, extras, publish/unpublish, merge duplicates |
| | **Cinema collections** | MANAGE_COLLECTIONS | Curated rows ("A good place to start," "For the love of cinema"), spotlight/hero picks |
| | **Cinema transcoding** | MANAGE_TRANSCODE | Workers, tokens, queue, failures, re-run |
| | **Live TV** | MANAGE_TV | Channels, guide imports, schedule preview, filler policies (cinema-live-tv spec) |
| Money | **Donations** | MANAGE_DONATIONS | Overview and the payout wallet (xpub, Esplora endpoint, confirmations; changing it needs a second factor) |
| | **Invoices** | MANAGE_DONATIONS | Invoices list, detail and manual resolution |
| | **Entitlements** | MANAGE_DONATIONS | Grants and revokes of Patron and server perks (a grant to the caller's own account or a server the caller owns is refused, D-574, D-581) |
| | **Pricing and perks** | MANAGE_DONATIONS | Prices (Patron, levels, invite credit), leaderboard moderation, **Cost Calculator** (doc 08 §6.4) |
| Platform | **Features** | MANAGE_FEATURES | Every feature module with platform-scope settings + kill switch. Two modules take the place of separate pages: **Themes & Defaults** (`platform.appearance`, the default theme per section, D-254) and **Email & Integrations** (`platform.integrations`: link-preview allow and deny hosts, GIF region and locale, metadata lookup providers; secrets and endpoints such as SMTP and the Discord application stay in `sr.toml`) |
| | **Limits & settings** | MANAGE_FEATURES | Platform settings that take effect without a restart, each save recorded with its reason: account cap, invite credits and thresholds, upload limits, message limits, rate limits, retention windows (deleted messages, large files, audit), storage, Activity package sizes and others (`crates/sr-admin/src/settings.rs`) |
| | **Entrance pages** | MANAGE_ENTRANCE | The `/no-invitation` text, the house notes, a landing note, the hero image and its alt text; saving needs a fresh second factor (D-239) |
| | **Announcements** | MANAGE_FEATURES | Platform-wide notices shown at the top of the app; broadcast and edits need a second factor |
| | **System** | OPERATOR | Version and readiness probes, disk, backups, storage, bridge and integration status (SMTP, Discord sign-in and others), preflight, CSP reports, the operator test mail |
| | **Arcade** | OPERATOR | Featuring a game, the Coming Soon showcase and its release |
| | **Runbook** | OPERATOR | The playbooks shipped with the build (`deploy/RUNBOOK.md`) |
| Audit | **Platform audit log** | VIEW_ACCESS_LOG | What staff changed, with the reason given: the `admin_action` rows of the access log |
| | **Private content** | READ_PRIVATE_CONTENT | Read a channel or DM, including deleted messages (within `retention.deleted_message_days`, default 30) and edit history. **Reason + 2FA required. Every access logged** |
| | **Access log** | VIEW_ACCESS_LOG (Owner by default) | Private-content accesses and admin actions; hash-chain verification status |
| | **Command line** | MANAGE_INVITES, APPOINT_STAFF, OPERATOR or VIEW_ACCESS_LOG (any) | The `sr-cli` operations and their Admin equivalents |

## 3. Staff Server channels

`sr-cli bootstrap` creates `#general`, a voice channel and the channels marked "seeded"; the others are created or posted to as noted (`crates/sr-cli/src/guilds.rs`, `crates/sr-server/src/staff_feed.rs`). The seeded list is one constant (`STAFF_CHANNELS`); a Staff Server that lacks one of them at server start (made before the list grew, or a channel deleted since) gets it created, once, in the same category and with no overwrites, like `#alerts` (D-667; a per-server advisory lock keeps two nodes from making it twice). Everyone in the Staff Server can read these channels.

| Channel | Fed by | Card actions |
|---|---|---|
| `#review-queue` (seeded) | review submissions | Approve · Request changes · Reject · Open review page; a package card has its own Approve |
| `#alerts` (seeded) | system | Suspicion events, payment anomalies, transcode failures, disk over its threshold (default 80%), backup failures, hash-chain break, failing jobs, delisted cartridges, takedown byte matches. Acknowledge · Open in Admin |
| `#crash-triage` | client crash reports | Created, staff-only, when the first crash card is posted (OPS-CRASH-001); no buttons that act |
| `#donations` (seeded) | payments | A card names no donor (staff with `MANAGE_DONATIONS` open the invoice to see who paid): Open invoice |
| `#new-members` (seeded) | registrations | "@new joined, invited by @issuer" with Open in invite graph |
| `#staff-announcements`, `#ops` (both seeded), voice `General` | humans (appointment notices go to `#staff-announcements`) | — |

## 4. Appointment flow (D-132)
1. A member with APPOINT_STAFF goes to Admin → Staff → Appoint, picks an account, starting roles, and a reason.
2. The appointee receives a system DM: "You've been invited to join SIDE ROOM Staff." [Accept] [Decline].
3. Accept → added to the Staff Server with the roles. `staff_appointments` row created. Audit logged. `#staff-announcements` system message.
4. Revocation (Admin → Staff → Revoke, or the member is removed from the Staff Server) → standing ends at once (an active appointment and Staff Server membership are both read on every request), and the open admin page closes in real time (`SR_STAFF_ALERT`).
5. The Owner cannot be revoked. Ownership transfer is CLI-only (`sr-cli staff transfer-owner`).

## 5. Acceptance criteria
- [x] Ordinary server invites to the Staff Server fail (create and redeem; `crates/sr-api/src/routes/settings/invites.rs:220`, `crates/sr-db/src/invite_join.rs:128`). "Invite to Server" is hidden there (`apps/web/src/lounge/GuildRail.tsx:114`, `ChannelSidebar.tsx:398`, `settings/server/sections/Invites.tsx:51`).
- [x] Granting a Staff Server role with a platform bit to a non-appointed account has no platform effect (server-side test: the `fake_staff` actor of `crates/sr-server/tests/authz_matrix.rs` is refused on every staff route).
- [x] Removing a staff member from the Staff Server revokes access to `/admin` APIs within 1s (immediately: standing is evaluated on every request).
- [x] Every Private Content Viewer request produces a hash-chained log row (`crates/sr-api/src/routes/admin/private.rs`). A tampered row is detected by the nightly verifier (`admin.access_log.verify`, `crates/sr-server/src/admin_jobs.rs`).
- [ ] All configurable numbers in docs 06/08/09 are editable in Admin → Limits/Donations without a restart. _(built: Limits & settings, `crates/sr-admin/src/settings.rs` (account cap, invite credits and thresholds, upload, message and rate limits, retention windows) and the donation Pricing page; left: no row-by-row audit that every number named in docs 06, 08 and 09 has a setting)_
