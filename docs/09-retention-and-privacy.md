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
| Account (deleted/deactivated/banned) | Forever (D-124). Profile scrubbed to "Deleted User" for deleted accounts | Staff see the full record and the Person link |
| Sessions, login IPs | Sessions until expiry. Login IP history 180 days (security) | Staff with `MANAGE_ACCOUNTS` |
| Voice/video media | Never recorded or stored. E2EE | Nobody |
| Watch/read history | Until the user clears it | Per privacy settings |
| Invoices / donations | Forever (accounting) | Staff with `MANAGE_DONATIONS` |
| Invite graph | Forever | Staff with `MANAGE_INVITES` |
| Staff access log | Forever, hash-chained | Owner and `VIEW_ACCESS_LOG` |
| Library submissions that were rejected | Files purged 14 days after rejection. Metadata kept | Reviewers |

Jobs (doc 02 §6) enforce purges: `purge_deleted_messages` (hourly), `expire_large_attachments` (hourly), `purge_rejected_submissions` (daily).

**Bulk-delete and channel/server deletion:** messages move to `user_deleted` with the same 30-day window. Deleting a server marks all its channels deleted. Server metadata is kept for staff.

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

## 4. Data export

Settings → Data & Privacy → **Request my data**. A Discord-style package (JSON + attachments index) is built by a job and offered as a download link for 7 days. It includes the user's messages, servers, settings, history, ratings, reviews, invites issued, and invoices.

## 5. Future: moderation & compliance (Phase 7)

Not built now (D-126). Reserved now so nothing needs a painful migration later:
- `moderation_holds` + `messages.hold_id` + `attachments` hold propagation.
- `reason_category` on platform bans (needed for D-031 "serious cause").
- Report buttons are designed into the message and profile menus but disabled behind feature flag `moderation.reports`.
- An operator-facing note in `12-deployment-and-operations.md` lists jurisdiction-dependent obligations (copyright takedown handling, illegal-content reporting, data-subject rights) to address before Phase 7.
