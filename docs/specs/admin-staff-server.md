# Spec — Admin & Staff Server

Phase 3 · Decisions: D-125, D-130–D-133, D-012, D-023, D-031 · Docs 07, 06, 08.

## 1. Two surfaces
1. **The Staff Server**: a normal-looking Lounge server (with a special frame in the rail) for staff conversation, queues, and alerts. Its bot-fed channels carry **action buttons** (Approve, Deny, Open) so most daily work never leaves chat.
2. **Admin pages** (`/admin/*`): full-screen, using the Server Settings layout and Lounge theme, reachable from the Staff Server header dropdown ("Platform Admin") and the user menu. Each page requires a specific platform permission. Pages a staff member lacks are hidden.

## 2. Admin navigation

| Group | Page | Permission | Contents |
|---|---|---|---|
| Overview | **Dashboard** | STAFF_BASIC | Accounts vs cap (gauge), online now, voice rooms, egress, storage, job backlog, review queue sizes, donations this month vs hosting cost, recent alerts |
| | **Setup Wizard** | OPERATOR | First-run checklist (doc 12 §2) |
| People | **Accounts** | MANAGE_ACCOUNTS | Search (username, email, id, Discord id), filters by state, detail drawer (profile, Person, sessions, IP history, invites issued, invited by, servers, entitlements, flags). Actions: deactivate, reactivate, force password reset, reset 2FA, rename, change email, grant badge |
| | **Persons** | MANAGE_ACCOUNTS | A Person's account chain, manual link/unlink of accounts, notes, suspicion level |
| | **Invite Graph** | MANAGE_INVITES | Interactive graph (force-directed WebGL; Rust graph layout in WASM for large trees), search, path-to-root, subtree stats, highlight banned nodes, suspicion overlay, credit grants/revokes, freeze/unfreeze invite creation, open codes list |
| | **Platform Bans** | PLATFORM_BAN | Ban with reason category (serious / standard) + note, unban, list. Serious bans trigger inviter suspicion (D-031) |
| | **Staff** | APPOINT_STAFF | Staff list, appoint (account picker + roles), revoke, delegation of appointment rights (Owner only) |
| Content | **Review Queues** | REVIEW_* | Tabs: Cinema, Library, Apps, Packages, Directory. See the submissions spec |
| | **Cinema Catalog** | PUBLISH_MEDIA | Title/season/episode editor, assets, renditions, re-transcode, subtitles, artwork, publish/unpublish, merge duplicates |
| | **Library Catalog** | PUBLISH_READING | Series/edition/chapter editor, page reorder, re-convert, merge |
| | **Collections & Spotlight** | MANAGE_COLLECTIONS | Curated rows ("A good place to start," "For the love of cinema"), spotlight/hero picks, Library reading lists |
| | **Live TV** | MANAGE_TV | Channels, guide imports, schedule preview, filler policies (cinema-live-tv spec) |
| | **Ingest Sources** | PUBLISH_* | Watched folders, storage imports, naming-convention tester |
| | **Transcoding** | MANAGE_TRANSCODE | Workers, tokens, queue, failures, re-run |
| Community | **Server Directory** | MANAGE_DIRECTORY | Approve or deny listings, categories, featured servers |
| | **Servers** | MANAGE_GUILDS_GLOBAL | Search all servers, view metadata, transfer ownership, delete abandoned servers |
| | **Private Content Viewer** | READ_PRIVATE_CONTENT | Read a channel or DM, including deleted (≤ 30 days) messages and edit history. **Reason + 2FA required. Every access logged** |
| Money | **Donations** | MANAGE_DONATIONS | Wallet (xpub, Esplora endpoint, confirmations), prices (Patron, levels, invite credit), invoices list + manual resolution, entitlements, leaderboards moderation, **Cost Calculator** (doc 08 §6.4) |
| Platform | **Features** | MANAGE_FEATURES | Every feature module with platform-scope settings + kill switch |
| | **Limits** | MANAGE_FEATURES | Account cap, invite credits, upload limits, message limits, rate limits, retention windows (30-day purge, 7-day large files) |
| | **Entrance** | MANAGE_FEATURES | Landing and login copy, hero image, house notes |
| | **Themes & Defaults** | MANAGE_FEATURES | Default theme per section |
| | **Email & Integrations** | OPERATOR | SMTP, Discord OAuth app, GIF provider, link-unfurl allowlist/denylist |
| | **System** | OPERATOR | Health, versions, backups status, storage usage, metrics links |
| Audit | **Platform Audit Log** | STAFF_BASIC | All admin actions |
| | **Access Log** | VIEW_ACCESS_LOG (Owner by default) | Private-content accesses; hash-chain verification status |

## 3. Staff Server channels (seeded)

| Channel | Fed by | Card actions |
|---|---|---|
| `#review-queue` | `sr-review` | Approve · Request changes · Reject · Open in Admin |
| `#alerts` | system | Suspicion events, payment anomalies, transcode failures, disk > 80%, backup failures, hash-chain break |
| `#donations` | `sr-payments` | New confirmed donations (honours donor anonymity in display, but staff with MANAGE_DONATIONS can reveal) |
| `#new-members` | registrations | "@new joined, invited by @issuer" with a graph link |
| `#staff-announcements`, `#general`, `#ops`, voice `Staff Room` | humans | — |

## 4. Appointment flow (D-132)
1. A member with APPOINT_STAFF goes to Admin → Staff → Appoint, picks an account, starting roles, and a reason.
2. The appointee receives a system DM: "You've been invited to join SIDE ROOM Staff." [Accept] [Decline].
3. Accept → added to the Staff Server with the roles. `staff_appointments` row created. Audit logged. `#staff-announcements` system message.
4. Revocation (Admin → Staff → Revoke, or the member is kicked/leaves) → appointment closed, sessions' staff claims dropped, admin pages closed in real time.
5. The Owner cannot be revoked. Ownership transfer is CLI-only (`sr-cli staff transfer-owner`).

## 5. Acceptance criteria
- [x] Ordinary server invites to the Staff Server fail (create and redeem; _backend done_). "Invite to Server" is hidden there (_client, open_).
- [x] Granting a Staff Server role with a platform bit to a non-appointed account has no platform effect (server-side test).
- [x] Removing a staff member from the Staff Server revokes access to `/admin` APIs within 1s (immediately: standing is evaluated on every request).
- [x] Every Private Content Viewer request produces a hash-chained log row. A tampered row is detected by the nightly verifier.
- [ ] All configurable numbers in docs 06/08/09 are editable in Admin → Limits/Donations without a restart.
