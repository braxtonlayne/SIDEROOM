# 07 — Permissions and Staff

Decisions: D-125, D-130–D-133, D-060.

## 1. Two permission planes

| Plane                    | Scope                  | Source of truth                                                                                                                                                            |
| ------------------------ | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Guild permissions**    | Inside a server        | Discord's model exactly: `@everyone` role → role permissions OR'd → channel overwrites (role deny/allow, then member deny/allow). `ADMINISTRATOR` bypasses. Owner bypasses |
| **Platform permissions** | The whole installation | Granted only through roles **in the Staff Server**, and only to accounts with an active staff appointment                                                                  |

`sr-core::permissions` implements both and is compiled to WASM for the client.

## 2. Guild permission bits

Use Discord's bits 0–50 with identical semantics (CREATE_INSTANT_INVITE … SEND_POLLS, USE_EXTERNAL_APPS, etc.). SIDE ROOM adds guild-level bits from `1 << 56`:

| Bit | Name                  | Meaning                                                                                                                                   |
| --- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 56  | `START_WATCH_PARTY`   | Start a watch party in this channel                                                                                                       |
| 57  | `CONTROL_WATCH_PARTY` | Control playback in someone else's party (when the host allows shared control)                                                            |
| 58  | `USE_LIBRARY_EMBEDS`  | Post rich Cinema/Library cards (default-granted, enforced at send: a message with a `<library:id>` or `<cinema:id>` card needs it, D-241) |
| 59  | `MANAGE_EXTENSIONS`   | Enable or disable approved extensions/themes for the server                                                                               |
| 60  | `USE_EXPRESSIONS_EXTERNALLY` | Use this server's emoji, stickers and sounds in other servers and DMs; default-granted, checked in this server for the member's roles here, no channel overwrite (D-376) |

### External expressions (D-375, D-376)

Emoji, stickers and soundboard sounds of a server are usable by its members everywhere (other servers, DMs, group DMs). Two gates apply, and neither implies the other:

1. **The target.** In a server channel the target channel must grant `USE_EXTERNAL_EMOJIS` (18), `USE_EXTERNAL_STICKERS` (37) or `USE_EXTERNAL_SOUNDS` (45), with Discord's semantics. In a DM or group DM `dm_permissions` already grants them.
2. **The source.** The sender must hold `USE_EXPRESSIONS_EXTERNALLY` (60) in the server the expression belongs to, computed with guild permissions for the member's roles there (owner and administrator bypass; channel overwrites never apply). The sender must still be a member of the source server, and the expression's own `available` flag and, for emoji, `role_ids` restriction must pass. A member who left the source server fails the gate.

Bit 60 is in the default `@everyone` set and migration 1360 backfilled it into every existing `@everyone` role, so nothing changes until a server turns it off. It is not a Patron perk. Application emoji (D-237) belong to no server and are untouched. Enforcement is at the write: see doc 05 (`GET /users/@me/expressions` and the three refusal shapes).

### Server 2FA requirement (D-223)

With a server's `mfa_level` set to 1 (owner only, `POST /guilds/{id}/mfa`), every action behind the elevated guild permissions needs a second factor proved on the session in the last 15 minutes: `KICK_MEMBERS`, `BAN_MEMBERS`, `ADMINISTRATOR`, `MANAGE_CHANNELS`, `MANAGE_GUILD`, `MANAGE_MESSAGES`, `MANAGE_ROLES`, `MANAGE_WEBHOOKS`, `MANAGE_THREADS`, `MANAGE_GUILD_EXPRESSIONS` and `MODERATE_MEMBERS` (AutoMod, community and safety settings and invite deletion included; invite creation and a member's own content excluded). A session that has a second factor and has not proved it gets `403 900004`; an account with no TOTP and no passkey gets `403 900123 MFA_ENROLLMENT_REQUIRED`. Bots pass when the owner of the application has a second factor enrolled and get `900123` otherwise. The route list is in doc 05 (server safety fields).

**Expression packs (D-282).** Emoji, sticker and sound packs from the Extensions page are added to and removed from a server with `MANAGE_GUILD_EXPRESSIONS` alone (`MANAGE_GUILD` works too), because that permission's whole job is the server's expressions and its holder can already upload the same files one at a time. Themes, recipes and server templates keep `MANAGE_GUILD`. The MFA gate above applies to both. Removing a pack deletes exactly the items it added; the server's own uploads are never touched.

## 3. Platform permissions

Stored as a separate bitfield on Staff Server roles (`roles.platform_permissions`, only meaningful when `guild.is_staff_guild`).

| Bit | Name                   | Grants                                                                                                                                                        |
| --- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0   | `STAFF_BASIC`          | Access the admin pages' read-only dashboards                                                                                                                  |
| 1   | `REVIEW_MEDIA`         | Review Cinema submissions                                                                                                                                     |
| 2   | `REVIEW_READING`       | Review Library submissions                                                                                                                                    |
| 3   | `PUBLISH_MEDIA`        | Upload and publish directly, edit or replace Cinema items                                                                                                     |
| 4   | `PUBLISH_READING`      | Same, for Library                                                                                                                                             |
| 5   | `DELETE_CONTENT`       | Unpublish or delete library items                                                                                                                             |
| 6   | `MANAGE_COLLECTIONS`   | Curated rows, spotlight, reading lists                                                                                                                        |
| 7   | `MANAGE_TV`            | Live TV channels, guides, imports                                                                                                                             |
| 8   | `REVIEW_APPS`          | Approve developer applications, app versions, privileged intents                                                                                              |
| 9   | `REVIEW_PACKAGES`      | Approve themes, extensions, profile layouts, activities                                                                                                       |
| 10  | `MANAGE_DIRECTORY`     | Approve servers for the Server Directory                                                                                                                      |
| 11  | `MANAGE_ACCOUNTS`      | Deactivate, reactivate, rename, reset 2FA, link Persons; read the staff-only archive of a deleted account's original identifiers (each view is logged, D-326) |
| 12  | `PLATFORM_BAN`         | Platform-ban and unban                                                                                                                                        |
| 13  | `MANAGE_INVITES`       | View the invite graph, grant or revoke credits, clear suspicion                                                                                               |
| 14  | `MANAGE_DONATIONS`     | View invoices, manual adjustments, leaderboard moderation                                                                                                     |
| 15  | `MANAGE_FEATURES`      | Toggle features, edit platform settings and limits                                                                                                            |
| 16  | `READ_PRIVATE_CONTENT` | **High-level only.** Read DMs, private channels, deleted messages, edit history (every access logged)                                                         |
| 17  | `MANAGE_GUILDS_GLOBAL` | Act as admin in any server (transfer ownership, delete abandoned servers)                                                                                     |
| 18  | `VIEW_ACCESS_LOG`      | View `staff_access_log`                                                                                                                                       |
| 19  | `APPOINT_STAFF`        | Appoint or revoke staff (Owner-delegated, D-132)                                                                                                              |
| 20  | `MANAGE_TRANSCODE`     | Manage transcode workers and the job queue                                                                                                                    |
| 21  | `OPERATOR`             | Infrastructure settings: storage, wallet, SMTP, backups                                                                                                       |
| 22  | `MANAGE_CASES`         | Open, assign, act on and close moderation cases; place and release holds (D-210, D-213)                                                                       |
| 23  | `MANAGE_TAKEDOWNS`     | The takedown register and the data-request queue (D-210)                                                                                                      |
| 24  | `MANAGE_ENTRANCE`      | The Entrance copy keys and the hero image, behind a fresh second factor (D-239); `MANAGE_FEATURES` no longer grants them, `OWNER` implies it                  |
| 63  | `OWNER`                | Implicit for the Staff Server owner. Everything                                                                                                               |

"High-level platform staff" (D-125) = any role holding `READ_PRIVATE_CONTENT`. Only the Owner can grant that bit.

## 4. The Staff Server

- Created by `sr-cli bootstrap`. `is_staff_guild = true`. Cannot be deleted. Ownership is transferable only through the operator CLI.
- Shows in the guild list with a distinct badge/frame.
- Default channels: `#staff-announcements`, `#review-queue` (bot-fed feed of submissions with Approve/Deny buttons), `#reports` (Phase 7), `#alerts` (suspicion, payment, transcode failures), `#general`, `#ops`, plus a voice channel.
- Default roles: `Owner`, `Senior Staff` (APPOINT_STAFF, READ_PRIVATE_CONTENT, …), `Moderator`, `Media Curator`, `Library Curator`, `App Reviewer`, `Support`.
  **Default role permission bits (D-162).** The table below is the default. `sr-cli bootstrap` instantiates these from the `staff_role_templates` table (migration 0130; the same values live in `sr_admin::rules::default_staff_roles` and a database test keeps them equal). Moderator and Senior Staff gained the case and takedown bits on 2026-10-15 (D-210; migration `0793_staff_role_templates_cases.sql`, G-64). Changing a default means a new migration; roles already created in a Staff Server are ordinary roles and can be edited.

| Role            | Bits     | Platform permissions                                                                                                                                                                                                                |
| --------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Support         | 10241    | STAFF_BASIC, MANAGE_ACCOUNTS, MANAGE_INVITES                                                                                                                                                                                        |
| App Reviewer    | 769      | STAFF_BASIC, REVIEW_APPS, REVIEW_PACKAGES                                                                                                                                                                                           |
| Library Curator | 85       | STAFF_BASIC, REVIEW_READING, PUBLISH_READING, MANAGE_COLLECTIONS                                                                                                                                                                    |
| Media Curator   | 203      | STAFF_BASIC, REVIEW_MEDIA, PUBLISH_MEDIA, MANAGE_COLLECTIONS, MANAGE_TV                                                                                                                                                             |
| Moderator       | 4201473  | STAFF_BASIC, MANAGE_DIRECTORY, MANAGE_ACCOUNTS, PLATFORM_BAN, MANAGE_CASES (D-210; was 7169, migration 0793)                                                                                                                        |
| Senior Staff    | 31178751 | Everything except OWNER, MANAGE_DONATIONS, VIEW_ACCESS_LOG and OPERATOR (so it includes APPOINT_STAFF, READ_PRIVATE_CONTENT, MANAGE_CASES, MANAGE_TAKEDOWNS and MANAGE_ENTRANCE; was 14401535, migration 0875; 1818623 before 0793) |
| Owner           | bit 63   | OWNER                                                                                                                                                                                                                               |

Enforcement rules built on these (also the default, D-162): only the Owner may grant `READ_PRIVATE_CONTENT`, `APPOINT_STAFF`, `VIEW_ACCESS_LOG` or `OPERATOR`; a delegate can grant only bits they hold; the Owner role is never assignable (transfer is `sr-cli staff transfer-owner`); only the Owner can suspend, ban or reset 2FA of a staff account.

- **Joining:** server invites to the Staff Server are disabled. The only way in is an **appointment**: a member with `APPOINT_STAFF` opens Admin → Staff → Appoint, picks an account, and selects starting roles. The appointee receives a system DM and accepts, then is added. Everything is logged. The offer is the normal path. The Owner may also appoint directly from Admin → Staff ("Appoint directly (emergency)", behind re-auth and logged); `sr-cli staff appoint` covers bootstrap, before any web session exists (D-197 as amended).
- **Leaving or removal** revokes staff status instantly: active admin sessions are invalidated and the gateway pushes `SR_STAFF_ALERT {revoked}`.
- Staff membership is shown as a "SIDE ROOM Staff" badge on profiles (configurable).

## 5. Staff access logging (D-125)

Any read of private content through admin tools:

1. Requires a **reason** field, plus a 2FA re-prompt if one hasn't happened in the last 15 minutes.
2. Writes `staff_access_log` (hash-chained: `hash = sha256(prev_hash || row)`).
3. The Owner sees the log in Admin → Access Log. A nightly job verifies the chain and alerts on any break.

## 6. Sensitive actions (RDY-SEC-010)

One list in code, `SENSITIVE_ACTIONS` in `crates/sr-api/src/sensitive.rs`, names every action that needs a fresh proof of identity. A staff row is enforced by a route layer (`reauth_gate`) that runs before the handler: the session must have proved a second factor in the last 15 minutes, otherwise `403 900004 SR_REAUTH_REQUIRED` (`900004` as well when no second factor is enrolled), and the client answers by calling `POST /admin/reauth` and retrying. A password row asks for the account password in the request body. The host row is a command that needs the server's config and database in hand. The test `routes::admin::tests::reauth_list_tests` walks every staff row through the real router with the window closed, open and expired; `sensitive::tests` checks that each listed route is in `route_matrix.tsv` as a staff or owner route.

`SR_ADMIN_REQUIRE_REAUTH=0` switches the window off for development and the end-to-end suite. The server refuses to start with it when `server.public_url` is not a development host (`localhost`, a loopback literal or a `*.localhost` name; D-357), the same rule as `SR_PREFLIGHT_INSECURE_TLS`. The gate checks the second factor before the handler's own permission check, so a staff member without the permission also meets `900004` first and, once the window is open, the handler's plain `403` (D-362); a caller who is not staff never sees `900004`, because the gate only speaks to platform staff. The web client prompts once and retries once, so that second `403` is shown as a permission refusal and does not reopen the dialog (`reauth_list_tests`, `p21withReauth.test.ts`). The private-content reads (`/admin/private/*`) keep their own gate, which logs the refusal as `denied`.

The table below is printed by `cargo test -p sr-api sensitive::tests::print_table -- --nocapture`.

| Action                     | Gate                           | Route or command                                                | Effect                                                    |
| -------------------------- | ------------------------------ | --------------------------------------------------------------- | --------------------------------------------------------- |
| Staff appointment          | second factor, last 15 minutes | `POST /admin/staff`                                             | Appoint a staff member                                    |
| Staff appointment          | second factor, last 15 minutes | `PUT /admin/staff/{account_id}/roles`                           | Change a staff member's roles                             |
| Staff appointment          | second factor, last 15 minutes | `DELETE /admin/staff/{account_id}`                              | Remove a staff member                                     |
| Staff appointment          | second factor, last 15 minutes | `POST /admin/staff/offers`                                      | Offer an appointment                                      |
| Staff appointment          | second factor, last 15 minutes | `PUT /admin/staff/{account_id}/delegation`                      | Delegate a permission                                     |
| Platform ban               | second factor, last 15 minutes | `POST /admin/accounts/{account_id}/ban`                         | Platform-ban the Person behind an account                 |
| Platform ban               | second factor, last 15 minutes | `POST /admin/persons/{person_id}/ban`                           | Platform-ban a Person                                     |
| Platform ban               | second factor, last 15 minutes | `POST /moderation/cases/{case_id}/ban`                          | Platform-ban the Person a case is about                   |
| Data-request outcome       | second factor, last 15 minutes | `POST /admin/data-requests/{request_id}/run`                    | Run an export, erasure or other data request              |
| Data-request outcome       | second factor, last 15 minutes | `POST /admin/data-requests/{request_id}/decline`                | Decline a data request                                    |
| Takedown uphold            | second factor, last 15 minutes | `POST /moderation/takedowns/{notice_id}/uphold`                 | Uphold a takedown notice                                  |
| Secret rotation            | host access                    | `sr-cli rotate-secret-key`                                      | Re-seal every stored secret under a new server.secret_key |
| Owner transfer             | second factor, last 15 minutes | `POST /admin/guilds/{guild_id}/transfer-ownership`              | Make another member the owner of a server                 |
| Application secret reset   | account password               | `POST /applications/{application_id}/reset-secret`              | Replace an application's client secret                    |
| Application secret reset   | account password               | `POST /applications/{application_id}/bot/reset-token`           | Replace a bot token                                       |
| Payout descriptor change   | second factor, last 15 minutes | `PUT /admin/donations/wallet`                                   | Set the wallet that receives donations                    |
| Account takeover surface   | second factor, last 15 minutes | `POST /admin/accounts/{account_id}/reset-2fa`                   | Reset an account's second factor                          |
| Account takeover surface   | second factor, last 15 minutes | `POST /admin/accounts/{account_id}/change-email`                | Change an account's email                                 |
| Account takeover surface   | second factor, last 15 minutes | `POST /admin/accounts/{account_id}/force-password-reset`        | Force a password reset                                    |
| Account takeover surface   | second factor, last 15 minutes | `POST /admin/persons/{person_id}/identities/{identity_id}/move` | Move an identity between Persons                          |
| Announcements and entrance | second factor, last 15 minutes | `POST /admin/announcements`                                     | Broadcast an announcement                                 |
| Announcements and entrance | second factor, last 15 minutes | `PATCH /admin/announcements/{announcement_id}`                  | Edit an announcement                                      |
| Announcements and entrance | second factor, last 15 minutes | `PUT /admin/entrance/copy/{key}`                                | Change the public entrance copy                           |
| Announcements and entrance | second factor, last 15 minutes | `DELETE /admin/entrance/copy/{key}`                             | Reset the public entrance copy                            |
| Announcements and entrance | second factor, last 15 minutes | `PUT /admin/entrance/hero`                                      | Change the entrance hero image                            |
| Announcements and entrance | second factor, last 15 minutes | `DELETE /admin/entrance/hero`                                   | Remove the entrance hero image                            |
| Announcements and entrance | second factor, last 15 minutes | `POST /admin/system/email-test`                                 | Send the operator test mail                               |

## 7. Developer roles

Not staff. "App developer" status is an account flag set when a developer application is approved (D-062). See doc 11.
