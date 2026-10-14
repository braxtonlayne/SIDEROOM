# Moderation and compliance (Phase 7 specification)

Status: **specification only. Nothing in sections 3 to 10 is built.** The hooks listed in section 2 exist now (XC-MOD-001, XC-MOD-002). Binding decisions: D-122 (held messages are never purged from staff view), D-125 (staff access logging), D-126 (moderation and compliance tooling is a later phase), D-023/D-030/D-031 (Person record, invite graph, suspicion), D-032 and D-184 (age gate). Where this document disagrees with `docs/01-decisions-log.md`, the log wins. Related: `docs/07-permissions-and-staff.md`, `docs/09-retention-and-privacy.md`, `docs/12-deployment-and-operations.md` section 9, `docs/15-threat-model.md`, `docs/specs/admin-staff-server.md`.

Naming: `guild` in code, API and database; "server" in UI copy and docs.

This spec describes tooling. It does not set policy. The operator decides what is allowed and what the legal duties of their jurisdiction are (`docs/12` section 9 is a reminder, not legal advice).

## 1. Goals and non-goals

Goals:
1. Let members tell staff about content or people that break the rules, with enough context for staff to act and without exposing the reporter.
2. Give staff one place to track a problem from first report to outcome (the **case**), with an audit trail.
3. Let the operator process a takedown (copyright or illegal content) quickly and prove what was done and when.
4. Let a person exercise data-subject rights (access, export, correction, erasure) in a way that respects holds and the permanent Person record (D-124, D-030).
5. Enforce the age and NSFW policy that already exists in product (D-032, D-184) with staff-side tooling.
6. Surface inviter suspicion (D-031) and ban-evasion signals to the right staff.

Non-goals: automated content classification, third-party moderation services (the platform never phones home, AGENTS rule 5), inspection of end-to-end encrypted voice or video (nothing is recorded, doc 09), legal advice.

## 2. What exists now (reserved hooks)

| Hook | Where | Notes |
|---|---|---|
| `moderation_holds` table | migration 0011 | `id`, `reason`, `created_by`, `case_ref` (free text until cases exist), `created_at` |
| `messages.hold_id` | migration 0012 | A held message gets no `purge_after` (D-122). Users can still delete it from user view |
| `attachments.hold_id`, `submissions.hold_id` | migration 0580 | The same hold can freeze an attachment or a library/cinema submission |
| `person_bans.category` (`serious` / `standard`) | migration 0131 | Severity. `serious` propagates suspicion to the inviter (D-031) |
| `person_bans.reason_category` | migration 0580 | Why a ban was made, from a fixed vocabulary (section 8). Nullable for older bans |
| Retention guards | `sr-server` jobs, `sr-db` chat repo | The message purge skips held messages and messages with a held attachment. The abandoned-upload sweep skips held attachments |
| `moderation.reports` flag | `sr-features`, default **off** | Report entries in message, user, channel, library and cinema surfaces render only when on. With the flag on and no backend, choosing Report opens a dialog saying reporting is not available. Nothing is sent |
| `moderation.audit_retention_days` | platform setting | Guild audit log retention, default 45 days (D-158) |

Rule for every future retention job (large-attachment expiry, rejected-submission file purge, any new sweep): **a row with a non-null `hold_id` is never deleted or expired, and its stored object is never removed.** A job that touches a table with `hold_id` must carry that predicate and a test that proves it.

## 3. Reports

### 3.1 Who and what can be reported

| Target | Entry point | Target id |
|---|---|---|
| Message (guild, DM, group DM, thread) | message menu | message id (+ channel id) |
| User | member list, profile, DM row, friends page | account id |
| Guild channel | channel context menu | channel id |
| Server | server menu | guild id |
| Library item | series page | series id |
| Cinema item | title page | title id |
| Other (invite, expression, app, theme) | later | typed id |

Reports are filed by any logged-in account that can see the target. A report on a DM message is allowed for a participant of that DM only.

### 3.2 Report form

Category (the section 8 vocabulary plus "something else"), a free-text note (up to 2000 characters), and an optional "also block this person". The client sends only ids and the note. **The server snapshots the evidence**: for a message, its content, attachments index, embeds and the ten messages before and after it, taken at report time, so deleting the message later does not erase the evidence. Filing a report places a **hold** (section 4.3) on the reported message and its attachments; user and channel targets have nothing to freeze.

### 3.3 Reporter privacy

- The reported person never sees who reported them. Server moderators see the reporter only if the reporter agreed; platform staff always see it.
- Reporter identity is access-logged like private content (D-125).
- Rate limit: reports per account per day (a platform setting), and one open report per (reporter, target).
- Reporting never auto-actions. Nothing is hidden or removed from the reported person until a human acts.
- Reporters get a status they can read (received, under review, closed) and a short outcome line, never the other party's details.

### 3.4 Routing

| Target lives in | Goes to |
|---|---|
| A guild | The guild's moderators (`MANAGE_MESSAGES` or a configured report channel), with an escalate button to platform staff |
| A DM or group DM | Platform staff only (no guild moderator may read DMs) |
| Library or cinema item | Reviewers (`REVIEW_READING` / `REVIEW_MEDIA`) and `DELETE_CONTENT` holders |
| A user outside any guild context, a server itself, anything flagged illegal content or child safety | Platform staff (`PLATFORM_BAN`, `MANAGE_ACCOUNTS`) |

Reports in the child-safety and illegal-content categories always go to platform staff regardless of where they were filed.

## 4. Cases

A **case** groups everything about one problem: one or more reports, evidence, holds, notes and actions. It is the unit staff work, and what `moderation_holds.case_ref` points at.

### 4.1 Data

`cases`: id, state, category, severity, subject (Person id and the account(s) involved), guild id (null for platform), assignee, opened_by (report or staff), opened_at, closed_at, outcome. `case_reports`, `case_evidence` (snapshots, immutable), `case_notes` (internal, append-only), `case_events` (timeline). In the Phase 7 migration `moderation_holds.case_ref` becomes a foreign key to `cases.id`; existing free-text refs are migrated or left null.

### 4.2 Lifecycle

`open` -> `triage` -> `investigating` -> `actioned` or `dismissed` -> `closed`. Reopening is allowed within a configurable window, then a new case links to the old one. Duplicate reports on the same target attach to the open case.

### 4.3 Holds

- Placing a hold needs a reason and a case. Releasing a hold needs a reason. Both are logged.
- Hold scope: a message (and its attachments and edit history), an attachment, a submission, or all of a user's messages in a window.
- A held message stays visible to high-level staff (`READ_PRIVATE_CONTENT`) indefinitely, even after the author deletes it (D-122); the author's user view still hides it.
- Releasing a hold on a row whose `purge_after` is long past makes it purgeable on the next job run; the job logs the count.
- Reading held private content is access-logged with a reason (D-125).

### 4.4 Actions

From a case, staff with the right bit can: delete or restore content, remove an attachment, warn, time out, kick, guild-ban, platform-ban (with `category` and `reason_category`), suspend an account, take down a library or cinema item, and place or release a hold. Every action writes a `case_events` row and the existing audit log (guild or platform). Server moderators can use only guild actions; platform actions need platform bits (doc 07 section 3).

### 4.5 Notifications to the subject

When an action is taken against a person, they receive a system DM stating what was done, the rule category, the duration, and how to appeal. It never names the reporter. Serious-category bans may skip the DM when staff judge it unsafe.

### 4.6 Appeals

Banned or suspended accounts cannot log in (D-124 keeps the account). The ban screen offers an appeal form that creates an appeal linked to the case, reviewed by a different staff member than the one who decided, one appeal per decision.

## 5. Takedown workflow

Two kinds: **copyright** notices and **illegal-content** notices (anything the operator's jurisdiction requires removing). Notices arrive at the contact address the operator publishes (`docs/12` section 9). Staff enter a notice in Admin -> Moderation -> Takedowns; there is no public form in v1.

1. **Intake.** Record notifier name and contact, claimed work or legal basis, exact URLs or ids, date received, the original text (attached), and who entered it. A case opens with `category = illegal_content` or a copyright category.
2. **Triage (target: within one business day).** Identify the content in SIDE ROOM (message, attachment, library series or chapters, cinema title or episode, media file). Reject incomplete notices with a templated reply.
3. **Hold, then disable.** Place a hold on the affected rows (so evidence and the stored object survive) and make the content unavailable: unpublish a library or cinema item, set a message to `staff_removed`, block the stored object on the media/CDN path. Held and removed are different: removed content stays out of every user view; held content stays in the evidence store.
4. **Notify the uploader or author.** System DM with the notice summary (not the notifier's personal data unless required), and the counter-notice path where the operator's policy has one.
5. **Counter-notice and restore.** Staff can record a counter-notice, set a restore-after date if the operator's policy uses one, and restore content. Restore is a logged action.
6. **Repeat-infringer tracking.** Each upheld takedown against an account is a strike event on the Person. An operator-configurable threshold (default off) opens a case for suspension review.
7. **Closure and retention.** Notices, evidence and outcome are kept for a configurable period (default forever, since the Person record is permanent, D-124). The takedown register is exportable (CSV/JSON).

The platform never judges whether a notice is valid in law. It records, holds, removes on staff action, and keeps the trail.

## 6. Data-subject requests

Extends the self-service export (`docs/09` section 4) and the deletion request stub (`account_delete_requests`, migration 0131).

| Right | Handling |
|---|---|
| Access / portability | The user's own "Request my data" export (JSON + attachments index, 7-day link). Staff can trigger it on a person's behalf after identity verification, logged |
| Rectification | Self-service profile and settings. Staff can correct Person-link errors with a logged reason |
| Erasure | The user starts deletion (D-029); the account becomes "Deleted User" and messages follow the 30-day purge (D-121). The Person record, invite graph, invoices and staff-side moderation history are **retained** (D-124, doc 09). The user is told this up front. Held rows are retained while held |
| Restriction / objection | Staff tool to hide an account's activity from discovery while a request is reviewed; no data is deleted |
| Request log | Each request is a `data_requests` row (kind, requester, verified by, state, due date, outcome). The Admin queue shows due dates against the operator's configured deadline |

When a deletion request meets a hold, the held rows are kept, the request outcome says why, and the hold's case is flagged for a decision (release, or retain with a stated legal basis). Staff, not the system, decide.

## 7. NSFW and age policy enforcement

Product behaviour already decided: a one-time 18+ confirmation stored on the account (D-032); age-restricted library titles are visible to everyone but opening one needs confirmation (D-184); age-restricted channels are marked `nsfw`.

Staff tooling to add:
- **Mark and unmark** content as age-restricted (series, titles, channels) from a case or the item page, logged; the change applies on the next request.
- **Misplaced NSFW**: a report category "adult content outside an age-restricted place". Actions: move or delete the message, mark the channel `nsfw`, warn the author.
- **Underage handling.** For an account found to be under 18 (self-declared or reported), staff can reset the age confirmation, which re-prompts. Whether being underage is a ban reason is the operator's call (open question 5). Child-safety reports always escalate (section 3.4) and carry the `child_safety` reason category.
- **Previews**: unfurls and embeds from age-restricted channels are not shown in places that do not require the age gate.
- **Dashboard metrics**: counts of age-restricted items, recent age-gate resets, open child-safety cases.

## 8. Ban reason taxonomy

`person_bans.category` stays the severity flag that drives suspicion. `reason_category` (enum `ban_reason_category`, migration 0580) is the why, picked in the ban dialog and used by reports and dashboards:

| Value | Meaning | Typical severity |
|---|---|---|
| `spam` | Unsolicited bulk content, scams | standard |
| `harassment` | Harassment, hate, threats | standard or serious |
| `illegal_content` | Content unlawful to host | serious |
| `child_safety` | Sexual content involving minors, grooming | serious |
| `ban_evasion` | Returning after a ban | standard or serious |
| `fraud` | Payment fraud, chargeback abuse, impersonation | serious |
| `platform_abuse` | Attacks on the service, scraping, exploit abuse | serious |
| `nsfw_policy` | Adult content outside allowed places | standard |
| `other` | A note is required | standard |

The severity staff choose is stored in `category`, which is what the invite graph reads, so changing the default mapping later never rewrites history. Adding a value is a forward-only migration.

## 9. Staff dashboards (Admin -> Moderation)

All views sit behind existing staff permissions; every read of private content follows D-125.

1. **Queue**: open cases by age, category, severity, assignee; filter by guild; claim and release.
2. **Reports inbox**: unattached reports, duplicate grouping, bulk dismiss with a reason.
3. **Case detail**: timeline, evidence snapshots, linked accounts and Person, holds with release, notes, actions, appeal.
4. **Holds list**: every active hold with its case, age and size of held content; a warning for holds older than a configurable limit so they are reviewed, not forgotten.
5. **Takedown register**: notices, states, due dates, strikes.
6. **Data requests**: queue with deadlines.
7. **Bans**: platform bans by `reason_category`, severity and age; lift with reason.
8. **Health**: reports per day, median time to first action, reopen rate, appeal outcomes. Aggregates only; no per-staff ranking by default.
9. **Retention check**: counts of rows skipped by purge jobs because of holds, so the operator sees what is accumulating.

## 10. Inviter-suspicion tooling

Builds on D-031 and doc 06 section 5.4, which already define the rule (a serious-cause ban of an invitee raises the inviter Person's `suspicion_level`; configurable freeze and escalation thresholds; staff can clear). Phase 7 adds:

- A **suspicion panel** on the Person page: the history (`suspicion_events`), banned descendants with their `reason_category`, and the invite path to the root.
- An **auto-opened case** when a threshold is crossed (invites frozen, escalation), assigned to senior staff and linked to the banning case.
- **Evasion signals** shown to staff, never auto-acted on: shared login IPs inside the 180-day window (doc 09), a matching Discord identity, shared invite lineage, a new account created shortly after a ban. A signal creates a review item, not a ban.
- A **clear with reason** action, logged and visible on the Person page; clearing does not delete events.
- The inviter is told only that invite creation is paused for review, never which invitee caused it.

## 11. Permissions

No new bit is required to start: reports use existing guild bits and the platform bits `PLATFORM_BAN`, `MANAGE_ACCOUNTS`, `MANAGE_INVITES`, `DELETE_CONTENT`, `READ_PRIVATE_CONTENT` and `VIEW_ACCESS_LOG`. If the owner wants separate `MANAGE_CASES` and `MANAGE_TAKEDOWNS` bits they would be added to doc 07 as new platform bits (open question 2).

## 12. Threat model additions (feed `docs/15`)

- Report abuse (mass false reports to silence someone): rate limits, dismissed-report counters per reporter, no automatic action.
- Retaliation against reporters: reporter identity hidden and access-logged.
- Evidence tampering: snapshots are immutable; holds prevent purge; case events are append-only.
- Staff abuse of access: reason + 2FA re-prompt + hash-chained access log (D-125).
- Hold leakage: held content is visible only to `READ_PRIVATE_CONTENT` holders.

## 13. Delivery plan

1. Cases, reports (guild-scoped), hold UI and release; turn `moderation.reports` on for a pilot guild.
2. Takedown register and library/cinema integration.
3. Data-request queue.
4. Dashboards and metrics.
5. Suspicion panel and evasion signals.

Each step ships behind its own flag and adds the retention-guard test for any purge it touches.

## 14. Open owner questions

1. Who triages reports in a server: moderators only, or platform staff too by default? Should reporter identity ever be visible to server moderators (section 3.3 proposes opt-in)?
2. Dedicated `MANAGE_CASES` and `MANAGE_TAKEDOWNS` platform bits, or reuse existing bits (section 11)?
3. Which `reason_category` values count as serious by default for D-031 (section 8 proposes `illegal_content`, `child_safety`, `fraud`, `platform_abuse`, with `harassment` and `ban_evasion` chosen by staff)? Is the list itself right?
4. Copyright: a public takedown form and a formal counter-notice process, or staff-entered notices only (section 5 proposes staff-entered)? Repeat-infringer threshold?
5. Underage accounts: is being under 18 a ban reason, or a reset-and-restrict action? Is there a minimum registration age at all?
6. Data-subject deadlines and jurisdiction: which regime must the request queue track, and what deadline does it show? Is keeping the Person record after erasure (D-124) acceptable under it, and should the deletion copy say so?
7. How long are closed cases, evidence snapshots and takedown notices kept (section 5 proposes forever, matching D-124)? Should holds expire or require periodic review?
8. May server moderators place holds on their own server's messages, or only platform staff (section 4.3 proposes platform staff only)?
9. May a platform reviewer see a reported DM message, given D-125 only covers staff reads with a reason? Proposed: yes, only the reported message and ten around it, snapshotted by the reporter's action.
10. Are appeals wanted at launch, and who reviews them when staff are few?
11. Should the web client get a flag-read endpoint now, so `moderation.reports` can be turned on per environment without a rebuild? Today the client reads a build-time seed.
