# Moderation and compliance (Phase 7 specification)

Status: **steps 1 and 2 of section 13 are built (P18, M-25 (a) and (b)): reports, cases, holds, appeals and the Admin > Moderation queue and case pages (sections 3, 4, 9 items 1, 3 and 4), and the takedown register with the strike counter, counter-notices and the library and cinema integration (section 5, notes in section 16). Sections 6, 7 (staff tooling), 9 items 2 and 5 to 9, and 10 are not built.** The item list is `docs/inventory/19-moderation.md`; the code is the crate `sr-moderation`, the routes under `/moderation` and the migrations `1090_moderation_cases.sql` and `1120_takedown_register.sql`. The hooks listed in section 2 exist (XC-MOD-001, XC-MOD-002). Binding decisions: D-122 (held messages are never purged from staff view), D-125 (staff access logging), D-126 (moderation and compliance tooling is a later phase), D-023/D-030/D-031 (Person record, invite graph, suspicion), D-032 and D-184 (age gate). Where this document disagrees with `docs/01-decisions-log.md`, the log wins. Related: `docs/07-permissions-and-staff.md`, `docs/09-retention-and-privacy.md`, `docs/12-deployment-and-operations.md` section 9, `docs/15-threat-model.md`, `docs/specs/admin-staff-server.md`.

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
| `moderation.reports` flag | `sr-features`, default **off** | Report entries in message, user, channel, server, library and cinema surfaces render only when on. Since step 1 (P18) choosing Report opens the report dialog, which sends `POST /moderation/reports`; the route answers 403 `SrFeatureDisabled` while the flag is off |
| `moderation_holds.case_id`, `scope_kind`, `scope_id`, `released_at`, `released_by`, `release_reason`, `reviewed_at` | migration 1090 | The hold implementation (section 4.3). `case_ref` stays for older free text |
| `moderation.reports_per_day`, `moderation.hold_review_days`, `moderation.case_reopen_days` | platform settings (Admin > Limits) | Reports per account per day (20), days before a hold is flagged for review (90), days a closed case can be reopened (30) |
| `takedown_notices`, `takedown_items`, `takedown_events`, `takedown_strike_reviews`, `blocked_objects`; `hold_id` on `reading_series`, `reading_chapters`, `video_titles`, `video_episodes` | migration 1120 | The takedown register (section 5, notes in section 16). A held catalogue row cannot be deleted (trigger); `moderation_holds.scope_kind` gains `library_series`, `library_chapter`, `cinema_title`, `cinema_episode` |
| `moderation.audit_retention_days` | platform setting | Guild audit log retention, default 45 days (D-158) |

Built in step 1: `sr-moderation::holds` is the one implementation. `place_hold_in` and `release_hold_in` take a transaction and say who acted (`None` is the system, as when a report freezes the reported message), for the takedown register and the data-request queue to call; `Moderation::place_hold` and `release_hold` add the platform-staff check (`MANAGE_CASES`, D-213) and the access-log row. The review housekeeping (draft abandonment and the purge of rejected submissions) now skips held submissions too.

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

- The reported person never sees who reported them. Server moderators do not see the reporter unless the reporter ticks "moderators may see my name" in the report form (off by default); platform staff always see it (D-209).
- Reporter identity, when platform staff open it, is access-logged like private content (D-125).
- Rate limit: reports per account per day (a platform setting), and one open report per (reporter, target).
- Reporting never auto-actions. Nothing is hidden or removed from the reported person until a human acts.
- Reporters get a status they can read (received, under review, closed) and a short outcome line, never the other party's details.

### 3.4 Routing

| Target lives in | Goes to |
|---|---|
| A guild | The guild's moderators (`MANAGE_MESSAGES` or a configured report channel) and only them, with an escalate button to platform staff. Platform staff see it once it is escalated (D-209) |
| A DM or group DM | Platform staff only (no guild moderator may read DMs) |
| Library or cinema item | Reviewers (`REVIEW_READING` / `REVIEW_MEDIA`) and `DELETE_CONTENT` holders |
| A user outside any guild context, a server itself, anything flagged illegal content or child safety | Platform staff (`PLATFORM_BAN`, `MANAGE_ACCOUNTS`) |

Reports in the child-safety and illegal-content categories (`child_safety`, `illegal_content`) are always escalated to platform staff regardless of where they were filed (D-209).

**Reported DMs (D-214).** A platform reviewer may read a reported DM message within the snapshot only: the reported message and the ten before and after it, taken at report time by the reporter's own action (a participant exposing a conversation they are part of). Opening the snapshot is access-logged with the case id as the reason. Reading anything beyond the snapshot needs `READ_PRIVATE_CONTENT` and the normal D-125 flow.

## 4. Cases

A **case** groups everything about one problem: one or more reports, evidence, holds, notes and actions. It is the unit staff work, and what `moderation_holds.case_ref` points at.

### 4.1 Data

`cases`: id, state, category, severity, subject (Person id and the account(s) involved), guild id (null for platform), assignee, opened_by (report or staff), opened_at, closed_at, outcome. `case_reports`, `case_evidence` (snapshots, immutable), `case_notes` (internal, append-only), `case_events` (timeline). In the Phase 7 migration `moderation_holds.case_ref` becomes a foreign key to `cases.id`; existing free-text refs are migrated or left null.

### 4.2 Lifecycle

`open` -> `triage` -> `investigating` -> `actioned` or `dismissed` -> `closed`. Reopening is allowed within a configurable window, then a new case links to the old one. Duplicate reports on the same target attach to the open case.

### 4.3 Holds

- Placing a hold needs a reason and a case, and is a platform-staff action (`MANAGE_CASES`, D-213): server moderators delete, time out, kick and ban inside their server, but a hold overrides the author's deletion rights (D-122) and is therefore not theirs to invoke. Releasing a hold needs a reason. Both are logged.
- Holds never expire by themselves and nothing silently unfreezes. A hold older than `moderation.hold_review_days` (default 90) is flagged for review on the holds dashboard (D-212).
- Hold scope: a message (and its attachments and edit history), an attachment, a submission, or all of a user's messages in a window.
- A held message stays visible to high-level staff (`READ_PRIVATE_CONTENT`) indefinitely, even after the author deletes it (D-122); the author's user view still hides it.
- Releasing a hold on a row whose `purge_after` is long past makes it purgeable on the next job run; the job logs the count.
- Reading held private content is access-logged with a reason (D-125).

### 4.4 Actions

From a case, staff with the right bit can: delete or restore content, remove an attachment, warn, time out, kick, guild-ban, platform-ban (with `category` and `reason_category`), suspend an account, take down a library or cinema item, and place or release a hold. Every action writes a `case_events` row and the existing audit log (guild or platform). Server moderators can use only guild actions; platform actions need platform bits (doc 07 section 3).

### 4.5 Notifications to the subject

When an action is taken against a person, they receive a system DM stating what was done, the rule category, the duration, and how to appeal. It never names the reporter. Serious-category bans may skip the DM when staff judge it unsafe.

### 4.6 Appeals

Appeals are wanted at launch, kept minimal (D-215). Banned or suspended accounts cannot log in (D-124 keeps the account). The ban screen offers an appeal form that creates an appeal linked to the case: one appeal per decision, reviewed by a different staff member than the one who decided; when nobody else holds the bit, the Owner reviews it; there is no second appeal.

## 5. Takedown workflow

Two kinds: **copyright** notices and **illegal-content** notices (anything the operator's jurisdiction requires removing). Notices arrive at the contact address the operator publishes (`docs/12` section 9). Staff enter a notice in Admin -> Moderation -> Takedowns; there is no public form in v1.

1. **Intake.** Record notifier name and contact, claimed work or legal basis, exact URLs or ids, date received, the original text (attached), and who entered it. A case opens with `category = illegal_content` or a copyright category.
2. **Triage (target: within one business day).** Identify the content in SIDE ROOM (message, attachment, library series or chapters, cinema title or episode, media file). Reject incomplete notices with a templated reply.
3. **Hold, then disable.** Place a hold on the affected rows (so evidence and the stored object survive) and make the content unavailable: unpublish a library or cinema item, set a message to `staff_removed`, block the stored object on the media/CDN path. Held and removed are different: removed content stays out of every user view; held content stays in the evidence store.
4. **Notify the uploader or author.** System DM with the notice summary (not the notifier's personal data unless required), and the counter-notice path where the operator's policy has one.
5. **Counter-notice and restore.** Staff can record a counter-notice, set a restore-after date if the operator's policy uses one, and restore content. Restore is a logged action.
6. **Repeat-infringer tracking.** Each upheld takedown against an account is a strike event on the Person. Default (MOD-4, owner's call, see section 14): staff-entered notices only, counter-notices recorded by staff, and `moderation.strike_review_threshold` = 3 upheld takedowns in 12 months opens a review case, never an automatic suspension. The threshold is operator-configurable.
7. **Closure and retention.** Closed cases, evidence snapshots and takedown notices are kept forever, consistent with the permanent Person record (D-124, D-212). The takedown register is exportable (CSV/JSON). The register and the data-request queue need `MANAGE_TAKEDOWNS` (D-210).

The platform never judges whether a notice is valid in law. It records, holds, removes on staff action, and keeps the trail.

## 6. Data-subject requests

Extends the self-service export (`docs/09` section 4) and the deletion request stub (`account_delete_requests`, migration 0131).

| Right | Handling |
|---|---|
| Access / portability | The user's own "Request my data" export (JSON + attachments index, 7-day link). Staff can trigger it on a person's behalf after identity verification, logged |
| Rectification | Self-service profile and settings. Staff can correct Person-link errors with a logged reason |
| Erasure | The user starts deletion (D-029); the account becomes "Deleted User" and messages follow the 30-day purge (D-121). The Person record, invite graph, invoices and staff-side moderation history are **retained** (D-124, doc 09). The user is told this up front. Held rows are retained while held |
| Restriction / objection | Staff tool to hide an account's activity from discovery while a request is reviewed; no data is deleted |
| Request log | Each request is a `data_requests` row (kind, requester, verified by, state, due date, outcome). The Admin queue shows due dates against the operator's configured deadline: `compliance.data_request_deadline_days`, default 30 (MOD-6 default, the owner names the jurisdiction) |

When a deletion request meets a hold, the held rows are kept, the request outcome says why, and the hold's case is flagged for a decision (release, or retain with a stated legal basis). Staff, not the system, decide.

## 7. NSFW and age policy enforcement

Product behaviour already decided: a one-time 18+ confirmation stored on the account (D-032); age-restricted library titles are visible to everyone but opening one needs confirmation (D-184); age-restricted channels are marked `nsfw`.

Staff tooling to add:
- **Mark and unmark** content as age-restricted (series, titles, channels) from a case or the item page, logged; the change applies on the next request.
- **Misplaced NSFW**: a report category "adult content outside an age-restricted place". Actions: move or delete the message, mark the channel `nsfw`, warn the author.
- **Underage handling.** For an account found to be under 18 (self-declared or reported), staff can reset the age confirmation, which re-prompts. Default (MOD-5, owner's call, see section 14): a minimum registration age of 13 (`accounts.minimum_age`, self-declared, stated on the invite landing); an account found to be under 18 is reset-and-restricted, not banned; one found under 13 is suspended with `reason_category = other` and a note, and the inviter is told only that the invitee did not meet the entry rules. Child-safety reports always escalate (section 3.4) and carry the `child_safety` reason category.
- **Previews**: unfurls and embeds from age-restricted channels are not shown in places that do not require the age gate.
- **Dashboard metrics**: counts of age-restricted items, recent age-gate resets, open child-safety cases.

## 8. Ban reason taxonomy

`person_bans.category` stays the severity flag that drives suspicion. `reason_category` (enum `ban_reason_category`, migration 0580) is the why, picked in the ban dialog and used by reports and dashboards:

| Value | Meaning | Typical severity |
|---|---|---|
| `spam` | Unsolicited bulk content, scams | standard |
| `harassment` | Harassment, hate, threats | standard (staff can raise it) |
| `illegal_content` | Content unlawful to host | serious |
| `child_safety` | Sexual content involving minors, grooming | serious |
| `ban_evasion` | Returning after a ban | standard (staff can raise it) |
| `fraud` | Payment fraud, chargeback abuse, impersonation | serious |
| `platform_abuse` | Attacks on the service, scraping, exploit abuse | serious |
| `nsfw_policy` | Adult content outside allowed places | standard |
| `other` | A note is required | standard |

Serious by default (D-211): `illegal_content`, `child_safety`, `fraud`, `platform_abuse`. `harassment` and `ban_evasion` default to standard and the ban dialog lets staff raise them; `spam`, `nsfw_policy` and `other` default to standard. These are presets only. The severity staff choose is stored in `category`, which is what the invite graph reads, so changing the default mapping later never rewrites history. Adding a value is a forward-only migration.

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

Two platform bits are added (D-210, doc 07 section 3): `MANAGE_CASES` (bit 22: open, assign, act on and close cases; place and release holds) and `MANAGE_TAKEDOWNS` (bit 23: the takedown register and the data-request queue). Moderator gains `MANAGE_CASES`; Senior Staff gains both. Reports otherwise use existing guild bits and the platform bits `PLATFORM_BAN`, `MANAGE_ACCOUNTS`, `MANAGE_INVITES`, `DELETE_CONTENT`, `READ_PRIVATE_CONTENT` and `VIEW_ACCESS_LOG`.

## 12. Threat model additions (feed `docs/15`)

- Report abuse (mass false reports to silence someone): rate limits, dismissed-report counters per reporter, no automatic action.
- Retaliation against reporters: reporter identity hidden and access-logged.
- Evidence tampering: snapshots are immutable; holds prevent purge; case events are append-only.
- Staff abuse of access: reason + 2FA re-prompt + hash-chained access log (D-125).
- Hold leakage: held content is visible only to `READ_PRIVATE_CONTENT` holders.

## 13. Delivery plan

1. **Built (P18).** Cases, reports (guild-scoped), hold UI and release, appeals; turn `moderation.reports` on for a pilot guild. The web client reads the flag from `GET /sr/features/flags` (D-216), so it can be turned on per environment without a rebuild.
2. **Built (P18).** Takedown register and library/cinema integration (section 16).
3. Data-request queue.
4. Dashboards and metrics.
5. Suspicion panel and evasion signals.

Each step ships behind its own flag and adds the retention-guard test for any purge it touches.

## 14. Owner questions: answered, and still open

Answered 2026-10-15 (docs/open-question-answers.md, MOD-1 to MOD-11). Adopted, folded into the sections above:

1. MOD-1, D-209: report routing and reporter visibility (sections 3.3 and 3.4).
2. MOD-2, D-210: the `MANAGE_CASES` and `MANAGE_TAKEDOWNS` bits (section 11, doc 07 section 3).
3. MOD-3, D-211: serious-by-default ban reasons (section 8).
7. MOD-7, D-212: closed cases and evidence are kept forever; holds are flagged for review after `moderation.hold_review_days` (sections 4.3 and 5).
8. MOD-8, D-213: holds are placed by platform staff only (section 4.3).
9. MOD-9, D-214: reported DM messages are readable within the snapshot (section 3.4).
10. MOD-10, D-215: minimal appeals at launch (section 4.6).
11. MOD-11, D-216: `GET /sr/features/flags` for `client_visible` platform flags, replacing the build-time `VITE_FEATURE_MODERATION_REPORTS` seed (section 13).

Still open, `NEEDS-OWNER` (legal), with the default that applies until the owner decides:

4. MOD-4, D-293, copyright: **built (P18, step 2; the answer is still a project default, not legal advice).** Staff-entered notices only; counter-notices recorded by staff; a review case at 3 upheld takedowns in 12 months (`moderation.strike_review_threshold`, `moderation.strike_window_months`, Admin > Features), never an automatic suspension; the content returns 10 business days after a counter-notice unless staff noted a court filing (section 5, notes in section 16). A lawyer should check the points named in D-293.
5. MOD-5, D-294 (p17: the setting `accounts.minimum_age` is real, registration records the affirmed age in `accounts.age_declared` and states it on the landing and the form), underage accounts: minimum age 13 by self-declaration (`accounts.minimum_age`); under 18 is reset-and-restrict; under 13 is a suspension (section 7). Since D-311 (M-21) the age confirmation is its own unticked checkbox on the registration form, separate from the house notes. Not legal advice: a lawyer should check the digital-consent age where the instance runs, whether self-declaration is enough there, that reset-and-restrict is the right handling for 13 to 17, and that a separate unchecked box is an acceptable form of the declaration.
6. MOD-6, D-295 (p17: `compliance.data_request_deadline_days`, `operator_country` and `operator_contact` are real settings; the privacy summary and `/help` show the operator details; the data-request queue that computes `due_at` from the deadline is part of the unbuilt Phase 7 backend), data-subject deadlines and jurisdiction: the queue shows a due date from `compliance.data_request_deadline_days` (default 30); the deletion screen and the data-export README say that the Person record, invite graph, invoices and staff-side moderation history are kept (D-124) and why (G-66). Whether that retention is lawful where the instance runs is the owner's call (section 6).

## 15. Implementation notes, step 1 (P18)

How step 1 reads the sections above, where the code chose a detail the text left open:

- **Tables** (migration 1090): `moderation_cases`, `moderation_reports`, `moderation_case_evidence`, `moderation_case_notes`, `moderation_case_events` (the timeline; actions are events), `moderation_appeals`, plus columns on `moderation_holds`. Evidence, notes and events refuse UPDATE and DELETE (trigger), so "kept forever" holds in the database, not by convention. No foreign key points at guilds or channels, so a case outlives a deleted server.
- **Two hats.** A server's moderator (`MANAGE_MESSAGES` in the case's server) and platform staff (`MANAGE_CASES`) use the same `/moderation/cases/{id}` routes; `rules::pick_who` picks the hat from the case's routing, and `rules::access` says what it may do (read, work, manage). Platform staff do not see a guild-routed case until its moderators escalate (D-209); after escalating, the moderators keep a list row and lose the detail. Holds, warnings, bans and appeals need `manage`, which only `MANAGE_CASES` gives.
- **Reporter privacy.** Case events never carry the reporter's id, so the timeline is safe for moderators to read. The reporter is returned to platform staff always (the read is an access-log row, `moderation.reporter.read`) and to server moderators only when the report ticked "moderators may see my name".
- **Always-escalated categories.** A `child_safety` or `illegal_content` report is routed to platform staff at once. When one attaches to a case a server is already working, the case flips to platform routing silently (`escalated_by` stays empty so the server's list does not show it).
- **System holds.** Reporting a message freezes it and its attachments with a hold whose `created_by` is empty. D-213 (holds are staff's) is about the manual tool; the report hold is the automatic freeze of section 3.2.
- **Releasing a hold on a message deleted while held** gives it a `purge_after` (deletion time plus `retention.deleted_message_days`), because deleting a held message clears it (D-122). It is purgeable at the next purge run.
- **Appeals** hang off a platform ban (`person_bans.id`, unique), so there is one per decision and no second one. A banned account has no session: `POST /moderation/appeals` is public and proves the account with its credentials under the sign-in throttles. The appeal joins the case that made the ban (a `platform_ban` case event names the ban id) or opens one.
- **Not built in step 1:** deleting or restoring content, timeouts, kicks and guild bans from a case (the case records that a moderator did them with the server's own tools), staff-opened cases from the Admin UI, the reporter's status page in Settings, the guild audit log entries for moderators' case actions (they are case events), a configured report channel, and the reviewers' own queue UI.

## 16. Implementation notes, step 2 (P18)

How step 2 reads section 5, where the code chose a detail the text left open:

- **Tables** (migration 1120): `takedown_notices` (the fields of step 1 of section 5, plus `triage_due_at`, `case_id`, the removal, counter-notice, court-filing and restore moments), `takedown_items` (one per piece of content, with what it looked like before, so a restore puts back exactly that), `takedown_events` (the notice timeline), `takedown_strike_reviews`, `blocked_objects`. Notices, items that were ever held and events are undeletable by trigger (D-212, kept forever).
- **States.** `received` -> `triage` (first content named) -> `removed` (upheld) or `rejected`; `removed` -> `counter_noticed` -> `restored`; a notice can also be `withdrawn` (before a decision or after) and restored by hand. Held and removed are two item states (`held`, `disabled`): `hold_notice` freezes for evidence without removing; `uphold_notice` holds what is not yet held and removes everything in one transaction.
- **Case category.** The notice opens a platform case with category `illegal_content` (severity serious) for both kinds, because the case vocabulary is the ban-reason one and has no copyright value; the notice `kind` says which. Both kinds are therefore always platform-routed (D-209).
- **Hold scopes.** `place_hold_in` freezes four more things: a library series (and its chapters), a library chapter, a cinema title (and its episodes), a cinema episode. The pages of a held chapter cannot be replaced either (trigger on `reading_pages`).
- **Disable.** Library and cinema items are unpublished (`published = FALSE`; the counters are recomputed as the catalogue code does) and put back only if they were published before. A message becomes `staff_removed` with no `purge_after` and goes back to `visible` on restore (the thread count follows). The live gateway event for a removed message is not sent: clients see it gone on the next fetch.
- **Media path.** An upheld notice writes `blocked_objects` (single keys, `library/{series}/` style prefixes, Cinema assets by id); the CDN route reads them through a cached copy (`MediaBlocklist`, 30 seconds) after the signature check and answers 451. The Cinema `transcode/` and download paths are not blocked by asset; the item is unpublished so no new links are minted.
- **Strikes.** A strike is a notice in `removed` or `counter_noticed` state whose `removed_at` lies inside `moderation.strike_window_months` (calendar months back from now), counted per Person over the uploader or author of its items. A restored or withdrawn notice no longer counts. Reaching the threshold opens one review case (platform, category `illegal_content`, timeline event `strike_review_opened`), at most one open review per Person. A library or cinema item with no creator on record has no Person and counts for nobody.
- **Business days.** `restore_after` is ten business days after the counter-notice, `triage_due_at` one business day after receipt. Weekdays follow the operator country (`compliance.operator_country`): Saturday and Sunday by default, Friday and Saturday for a short list of countries (Gulf states, Egypt, Israel and so on), Friday for Iran and Afghanistan, Friday and Sunday for Brunei. No holiday calendar (D-293).
- **Housekeeping.** Job `moderation.takedown_restore` (singleton id 1120, hourly) restores every notice whose wait is over and that has no court filing noted. Its access-log row (`moderation.takedown.auto_restore`) and the hold release are attributed to the staff member who recorded the counter-notice, marked `housekeeping`.
- **Uploader DM.** Each uploader or author gets one system DM with the notice reference, the claimed basis (cut at 300 characters) and the counter-notice path (the operator contact when set). It never carries the notifier's name or contact. A second DM tells them when the content returns.
- **Item pages.** `GET /moderation/item-status` (any signed-in person) answers `none`, `held` or `taken_down` and nothing else; the Library reader and Cinema player show a banner while an item is held and their own message once it is taken down (they would otherwise only see a missing page).
- **Not built in step 2:** the dashboard view of the register (`MOD-DASH-005`), blocking the Cinema `transcode/` objects by asset, a public form (none in v1, D-293), mailing the templated reply to the notifier (staff copy it), and the gateway event for a message removed by a takedown.
