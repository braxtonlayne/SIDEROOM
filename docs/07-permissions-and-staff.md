# 07 — Permissions and Staff

Decisions: D-125, D-130–D-133, D-060.

## 1. Two permission planes

| Plane | Scope | Source of truth |
|---|---|---|
| **Guild permissions** | Inside a server | Discord's model exactly: `@everyone` role → role permissions OR'd → channel overwrites (role deny/allow, then member deny/allow). `ADMINISTRATOR` bypasses. Owner bypasses |
| **Platform permissions** | The whole installation | Granted only through roles **in the Staff Server**, and only to accounts with an active staff appointment |

`sr-core::permissions` implements both and is compiled to WASM for the client.

## 2. Guild permission bits

Use Discord's bits 0–50 with identical semantics (CREATE_INSTANT_INVITE … SEND_POLLS, USE_EXTERNAL_APPS, etc.). SIDE ROOM adds guild-level bits from `1 << 56`:

| Bit | Name | Meaning |
|---|---|---|
| 56 | `START_WATCH_PARTY` | Start a watch party in this channel |
| 57 | `CONTROL_WATCH_PARTY` | Control playback in someone else's party (when the host allows shared control) |
| 58 | `USE_LIBRARY_EMBEDS` | Post rich Cinema/Library cards |
| 59 | `MANAGE_EXTENSIONS` | Enable or disable approved extensions/themes for the server |

## 3. Platform permissions

Stored as a separate bitfield on Staff Server roles (`roles.platform_permissions`, only meaningful when `guild.is_staff_guild`).

| Bit | Name | Grants |
|---|---|---|
| 0 | `STAFF_BASIC` | Access the admin pages' read-only dashboards |
| 1 | `REVIEW_MEDIA` | Review Cinema submissions |
| 2 | `REVIEW_READING` | Review Library submissions |
| 3 | `PUBLISH_MEDIA` | Upload and publish directly, edit or replace Cinema items |
| 4 | `PUBLISH_READING` | Same, for Library |
| 5 | `DELETE_CONTENT` | Unpublish or delete library items |
| 6 | `MANAGE_COLLECTIONS` | Curated rows, spotlight, reading lists |
| 7 | `MANAGE_TV` | Live TV channels, guides, imports |
| 8 | `REVIEW_APPS` | Approve developer applications, app versions, privileged intents |
| 9 | `REVIEW_PACKAGES` | Approve themes, extensions, profile layouts, activities |
| 10 | `MANAGE_DIRECTORY` | Approve servers for the Server Directory |
| 11 | `MANAGE_ACCOUNTS` | Deactivate, reactivate, rename, reset 2FA, link Persons |
| 12 | `PLATFORM_BAN` | Platform-ban and unban |
| 13 | `MANAGE_INVITES` | View the invite graph, grant or revoke credits, clear suspicion |
| 14 | `MANAGE_DONATIONS` | View invoices, manual adjustments, leaderboard moderation |
| 15 | `MANAGE_FEATURES` | Toggle features, edit platform settings and limits |
| 16 | `READ_PRIVATE_CONTENT` | **High-level only.** Read DMs, private channels, deleted messages, edit history (every access logged) |
| 17 | `MANAGE_GUILDS_GLOBAL` | Act as admin in any server (transfer ownership, delete abandoned servers) |
| 18 | `VIEW_ACCESS_LOG` | View `staff_access_log` |
| 19 | `APPOINT_STAFF` | Appoint or revoke staff (Owner-delegated, D-132) |
| 20 | `MANAGE_TRANSCODE` | Manage transcode workers and the job queue |
| 21 | `OPERATOR` | Infrastructure settings: storage, wallet, SMTP, backups |
| 22 | `MANAGE_CASES` | Open, assign, act on and close moderation cases; place and release holds (D-210, D-213) |
| 23 | `MANAGE_TAKEDOWNS` | The takedown register and the data-request queue (D-210) |
| 63 | `OWNER` | Implicit for the Staff Server owner. Everything |

"High-level platform staff" (D-125) = any role holding `READ_PRIVATE_CONTENT`. Only the Owner can grant that bit.

## 4. The Staff Server

- Created by `sr-cli bootstrap`. `is_staff_guild = true`. Cannot be deleted. Ownership is transferable only through the operator CLI.
- Shows in the guild list with a distinct badge/frame.
- Default channels: `#staff-announcements`, `#review-queue` (bot-fed feed of submissions with Approve/Deny buttons), `#reports` (Phase 7), `#alerts` (suspicion, payment, transcode failures), `#general`, `#ops`, plus a voice channel.
- Default roles: `Owner`, `Senior Staff` (APPOINT_STAFF, READ_PRIVATE_CONTENT, …), `Moderator`, `Media Curator`, `Library Curator`, `App Reviewer`, `Support`.
**Default role permission bits (D-162).** The table below is the default. `sr-cli bootstrap` instantiates these from the `staff_role_templates` table (migration 0130; the same values live in `sr_admin::rules::default_staff_roles` and a database test keeps them equal). Moderator and Senior Staff gained the case and takedown bits on 2026-10-15 (D-210; migration `0793_staff_role_templates_cases.sql`, G-64). Changing a default means a new migration; roles already created in a Staff Server are ordinary roles and can be edited.

| Role | Bits | Platform permissions |
|---|---|---|
| Support | 10241 | STAFF_BASIC, MANAGE_ACCOUNTS, MANAGE_INVITES |
| App Reviewer | 769 | STAFF_BASIC, REVIEW_APPS, REVIEW_PACKAGES |
| Library Curator | 85 | STAFF_BASIC, REVIEW_READING, PUBLISH_READING, MANAGE_COLLECTIONS |
| Media Curator | 203 | STAFF_BASIC, REVIEW_MEDIA, PUBLISH_MEDIA, MANAGE_COLLECTIONS, MANAGE_TV |
| Moderator | 4201473 | STAFF_BASIC, MANAGE_DIRECTORY, MANAGE_ACCOUNTS, PLATFORM_BAN, MANAGE_CASES (D-210; was 7169, migration 0793) |
| Senior Staff | 14401535 | Everything except OWNER, MANAGE_DONATIONS, VIEW_ACCESS_LOG and OPERATOR (so it includes APPOINT_STAFF, READ_PRIVATE_CONTENT, MANAGE_CASES and MANAGE_TAKEDOWNS; was 1818623, migration 0793) |
| Owner | bit 63 | OWNER |

Enforcement rules built on these (also the default, D-162): only the Owner may grant `READ_PRIVATE_CONTENT`, `APPOINT_STAFF`, `VIEW_ACCESS_LOG` or `OPERATOR`; a delegate can grant only bits they hold; the Owner role is never assignable (transfer is `sr-cli staff transfer-owner`); only the Owner can suspend, ban or reset 2FA of a staff account.

- **Joining:** server invites to the Staff Server are disabled. The only way in is an **appointment**: a member with `APPOINT_STAFF` opens Admin → Staff → Appoint, picks an account, and selects starting roles. The appointee receives a system DM and accepts, then is added. Everything is logged.
- **Leaving or removal** revokes staff status instantly: active admin sessions are invalidated and the gateway pushes `SR_STAFF_ALERT {revoked}`.
- Staff membership is shown as a "SIDE ROOM Staff" badge on profiles (configurable).

## 5. Staff access logging (D-125)

Any read of private content through admin tools:
1. Requires a **reason** field, plus a 2FA re-prompt if one hasn't happened in the last 15 minutes.
2. Writes `staff_access_log` (hash-chained: `hash = sha256(prev_hash || row)`).
3. The Owner sees the log in Admin → Access Log. A nightly job verifies the chain and alerts on any break.

## 6. Developer roles

Not staff. "App developer" status is an account flag set when a developer application is approved (D-062). See doc 11.
