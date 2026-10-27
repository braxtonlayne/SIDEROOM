# 09 — Retention and Privacy

Decisions: D-029, D-043, D-053, D-120–D-126.

## 1. Retention matrix

| Data | Retention | Who can see it after "deletion" |
|---|---|---|
| Messages (not deleted) | Forever | — |
| Message deleted by its author or a server moderator | Hidden from all user views immediately. **Purged 30 days later** (`purge_after`) | High-level staff (`READ_PRIVATE_CONTENT`) until purge |
| Message under a moderation hold | Never purged while held. Users may still delete it from user view | High-level staff, indefinitely |
| Message edit history | Kept while the message exists. Purged with the message | High-level staff only |
| Attachments ≤ 30 MB (images, files, voice messages) | Same as the parent message: "part of the message" | Same as the message |
| Attachments > 30 MB ("large", donor/level perk) | **Expire 7 days after upload**. The message shows an "This file has expired" chip. The object is purged | Nobody after expiry, unless held |
| Account (deleted/deactivated/banned) | Forever (D-124). After erasure the member is told what stays: the Person record, invite graph, invoices, staff-side moderation history, messages, uploads, reactions and poll votes they did not delete (shown as Deleted User, D-029, D-123, D-332), content under a hold, the linked Discord identity (D-030), a staff-only archive of the original username, email address and display name that staff may view only through a logged read (D-326), and the request log (D-318); an erasure run may also delete the member's messages when the member asked for it (N-06); session and sign-in addresses are blanked. Profile scrubbed to "Deleted User" for deleted accounts; the avatar, the banner and the profile page pictures (`profile_media`, D-260) are deleted with it and the data export lists the pictures | Staff see the full record and the Person link |
| Sessions, login IPs | Sessions until expiry. Login IP history 180 days (security), deleted by the daily job `moderation.suspicion_housekeeping` (P19); no hold exception, the finding is kept on the case instead (D-347) | Staff with `MANAGE_ACCOUNTS` |
| Voice/video media | Never recorded or stored. E2EE | Nobody |
| Watch/read history | Until the user clears it | Per privacy settings |
| Invoices / donations | `compliance.invoice_retention_years` (default 10, N-07, D-325) after the end of the year of payment, then detached from the member by the monthly job `payments.detach_expired_invoices` (N-08): the row, amounts, dates and chain address stay for the books, the account link is set to null | Staff with `MANAGE_DONATIONS` |
| Invite graph | Forever | Staff with `MANAGE_INVITES` |
| Staff access log | Forever, hash-chained | Owner and `VIEW_ACCESS_LOG` |
| Library submissions that were rejected | Files purged 14 days after rejection. Metadata kept | Reviewers |
| Storage mount (a member's or a server's own bucket; BYOS-LEG-001) | Until the member or the server owner removes it. The secret access key is stored sealed (AES-256-GCM under `server.secret_key`), never returned by any API, never in a log or a package. **Erasure of the account soft-deletes the member's mounts and wipes the sealed secret and the key id** | Staff with `MANAGE_TAKEDOWNS` see the mount list (owner, label, provider, bucket, status, key hint; never the secret) |
| The objects in a member's bucket | Never stored, copied or deleted by the instance: the bucket is the member's. A takedown **delists** an object (the instance stops showing it and refuses links to it); a counter-notice restores it | The member, and whoever the member shared the shelf with |
| Delisting records (takedown items of kind `storage_object`, whole-mount delists) | Forever with the notice (D-212); the access log records every staff delist and relist | Staff with `MANAGE_TAKEDOWNS` |
| Presigned links to a member's bucket | Minutes (the presign TTL, roadmap section 2); never stored, never put in a message or a crawlable document | The holder of the link until it expires |

Jobs (doc 02 §6) enforce purges (and `payments.detach_expired_invoices`, monthly, N-08): `purge_deleted_messages` (hourly), `expire_large_attachments` (hourly), `purge_rejected_submissions` (daily).

**Member self-delete (N-05, D-324):** Settings, Privacy, "Delete all my messages" hides every message and upload the member wrote and removes their reactions and poll votes (a closed poll keeps its totals as anonymous counts, rows under a hold stay, D-332), in servers (including ones they left) and in direct messages, with an optional cut-off date, as a job (one run at a time, once per 24 hours). Same window: `user_deleted`, purged 30 days later, never while under a hold; the run is written to the access log as `messages.self_delete`, and staff can ask for the same run on an erasure request (N-06). **Bulk-delete and channel/server deletion:** messages move to `user_deleted` with the same 30-day window. Deleting a server marks all its channels deleted. Server metadata is kept for staff.

**Data requests and mounts (BYOS-LEG-001):** a data package gains `storage.json`: the member's own mounts (label, provider, bucket, prefix, status, counts of delisted objects, and who each shelf is shared with) and the shelves friends shared with them. It never carries a key, a secret or an object key. Erasure reports how many mounts it removed in the request's outcome.

**Search engines (D-384, OPS-IDX-001):** only the landing page is indexable; `robots.txt` allows `/` alone, every other response carries `X-Robots-Tag: noindex, nofollow` and every other SPA route carries `<meta name="robots" content="noindex">`, so no member, profile, message or library item is ever discoverable through a search engine.

## 2. Encryption

| Path | Protection |
|---|---|
| Client ↔ server (HTTP, WS, HLS) | TLS 1.3 (hop-by-hop). The server can read text |
| Voice/video/screen share (DMs, group DMs, voice channels, streams, stage) | **End-to-end**: MLS group key agreement + per-frame AES-GCM (see voice spec). The SFU and TURN see only ciphertext |
| Data at rest | Disk encryption at the host level (LUKS) recommended. Postgres and Garage contents are otherwise plaintext to the operator |
| Passwords | argon2id |
| Tokens | Stored as SHA-256 hashes |

## 3. Activity privacy (D-120)

Settings → Activity Privacy:
- **Share my watching & reading activity**: default **ON**. Visible to **friends and people who share a server with you**, never to strangers.
- Per-section toggles (Cinema / Library / Live TV).
- "Hide age-restricted titles from my activity": default ON.
- Reviews and ratings are public to all logged-in members regardless of these toggles (D-120). Users can delete their reviews.
- "Pause history" (incognito): nothing recorded while on.
- Discord-parity toggles: display current activity as a status message, allow DMs from server members, message requests, friend requests from (everyone / friends of friends / server members), and so on.
- **Reading presence audiences (D-229).** `library.share_reading` is `off`, `friends` (default) or `everyone` (friends and people who share a server, never strangers), under the D-190 master switches; age-restricted series are masked beyond friends.
- **Sensitive media (D-230).** `sensitive_media` has three scopes with defaults: `friends` show (no signal until a scanner hook exists), `others` blur (image and video attachments in DMs from non-friends), `servers` show (attachments in age-restricted channels). `blur` is the spoiler flag for that viewer; `block` delivers the attachment without a URL and with `sr_filtered: true`; nothing is deleted and nothing is scanned.
- **Incident actions (D-243).** A moderator can pause direct messages between members for up to 24 hours (`PUT /guilds/{id}/incident-actions`, `dms_disabled_until`). A DM between two people who are not friends is then refused with `50007` when every server they share has the pause running. A pause blocks; it does not move the conversation to Message Requests, which stays the standing `dm_spam_filter` of D-224. Friends are never affected, an ended pause changes nothing, and nothing pauses DMs by itself. Each member can also turn DMs from one server off for themselves with the per-server override of `GET|PATCH /users/@me/privacy` (`server_dm_overrides`), which the Privacy Settings entry of the server menu and the per-server profile dialog expose.

## 4. Data export

Settings → Data & Privacy → **Request my data**. A Discord-style package (JSON, an attachments index, and the member's own reactions and poll votes, N-15) is built by a job and offered as a download link for 7 days. It includes the user's messages, servers, settings, history, ratings, reviews, invites issued, and invoices.

The due date of a data-subject request is computed from the `compliance.data_request_deadline_days` setting (default 30, `platform.compliance`, M-06, D-295) when the request is created (Admin, Data requests, M-25 step c; one documented extension of up to 60 days (days, not calendar months, D-316) may be added once, with a reason that the member is sent by system DM when reachable (`member_notified_at`; an extension granted after the original due date is marked late, D-317), and the queue marks requests past their date as overdue; `data_requests` rows are kept forever like the Person record, D-124); the operator's country and contact address, when set there, appear on the privacy summary (`/house-notes#privacy`) and on `/help`.

**The member handbook (RDY-ONB-002).** `/help` repeats the figures of this document in plain words: 30 days to purge a deleted message, 7 days for a file over 30 MB, 180 days for sign-in addresses, 7 days for the export link, and what stays after an account is deleted. `apps/web/src/help/handbook.test.tsx` reads this file and fails when a figure drifts, so change both together.

## 5. Future: moderation & compliance (Phase 7)

Not built now (D-126); the full Phase 7 design is `docs/specs/moderation-compliance.md`. Reserved now so nothing needs a painful migration later:
- `moderation_holds` + `messages.hold_id` + `attachments.hold_id` + `submissions.hold_id` (migrations 0011, 0012, 0580). Purge jobs skip held rows, including the review housekeeping of held submissions (P18). Holds are placed and released through `sr-moderation` (migration 1090 adds the case link and the release fields); releasing a hold on a message deleted while held gives it a `purge_after`, so it is purgeable at the next run.
- Daily job `moderation.suspicion_housekeeping` (P19, singleton id 1150): deletes `login_events` older than 180 days in slices of 20,000 rows (at most 25 slices per run, then it runs again after five minutes until the backlog is gone) and scans recent sign-ins for evasion signals. `login_events` has no `hold_id` by decision (D-347): a sign-in row is never kept for a case, and a confirmed evasion signal keeps its finding (counts and dates, never an address) on the signal and as evidence on the case. Each run records its count on its own job row (`jobs.payload`: `last_purged`, `last_run_at_ms`, `backlog`); the staff dashboard's retention check shows it with the number of rows still past the window (MOD-DASH-009, spec section 17.4). The purge reads nothing the other phase-19 tools keep: data exports are taken at run time, a data-subject restriction hides activity and keeps sign-ins on the same 180 days, and a confirmed signal's snapshot never held an address.
- Takedown register (step 2, P18; migration 1120): `takedown_notices`, their items and events, and the strike reviews are kept forever and cannot be deleted (trigger). A takedown holds the content through the same hold API (`reading_series`, `reading_chapters`, `video_titles`, `video_episodes` gained `hold_id`; a held catalogue row cannot be deleted, so its stored files cannot be queued for collection), makes it unavailable and blocks its stored objects on the media path until the notice ends. The strike window is `moderation.strike_window_months`; the content returns 10 business days after a counter-notice unless a court filing is noted.
- `reason_category` on platform bans (migration 0580; taxonomy in `docs/specs/moderation-compliance.md` section 8), alongside the `serious`/`standard` severity needed for D-031.
- Report buttons are designed into the message and profile menus but disabled behind feature flag `moderation.reports`.
- An operator-facing note in `12-deployment-and-operations.md` lists jurisdiction-dependent obligations (copyright takedown handling, illegal-content reporting, data-subject rights) to address before Phase 7.
