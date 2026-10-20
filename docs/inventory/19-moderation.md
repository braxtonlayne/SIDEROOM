# 19 — Moderation & compliance (`MOD`)

Phase 7. Spec: [moderation-compliance.md](../specs/moderation-compliance.md). Decisions: D-122, D-125, D-209 to D-216, D-293 to D-295, D-315. Owner round: M-25 in [answers-followups-p16.md](../dev/answers-followups-p16.md). Reserved hooks that already exist are `XC-MOD-001` to `XC-MOD-003` in [18-cross-cutting.md](18-cross-cutting.md).

Delivery order (spec section 13, D-315): step 1 = `MOD-RPT`, `MOD-CASE`, `MOD-HOLD`, `MOD-APL`, `MOD-PERM`; step 2 = `MOD-TKD`; step 3 = `MOD-DSR`; step 4 = `MOD-DASH` (later); step 5 = `MOD-SUS` (later). `MOD-AGE` and `MOD-BAN` are spec sections 7 and 8, listed so nothing in the spec is missing; they have no step yet. Each step has its own branch and flag.

## MOD-PERM — Permissions (spec section 11)
- [x] **MOD-PERM-001** [DB][BE] Platform bits `MANAGE_CASES` (22) and `MANAGE_TAKEDOWNS` (23); Moderator gains `MANAGE_CASES`, Senior Staff both (D-210) _(migration 0793, `sr-core` platform_permissions, `sr-admin` rules; built before this inventory.)_
- [x] **MOD-PERM-002** [BE] Every `/moderation` staff route checks its bit server-side and writes denials to the access log _(routes: `MANAGE_CASES`; takedown register and data requests: `MANAGE_TAKEDOWNS`.)_ _(sr-api `routes/moderation` (`Actor::staff_only`, `staff_result` writes a `denied` access-log row for a missing bit); tests in `routes/moderation/tests.rs`.)_

## MOD-RPT — Reports (spec section 3)
- [x] **MOD-RPT-001** [DB] `moderation_reports` (reporter, target kind and id, category, note, state, outcome line) with one open report per (reporter, target) _(migration 1090 `moderation_reports` with the partial unique index on (reporter, target) while not closed.)_
- [x] **MOD-RPT-002** [BE] `POST /moderation/reports` behind the `moderation.reports` flag: category from the section 8 vocabulary plus `other`, note up to 2000 characters, optional "also block", visibility check on the target (a DM report only by a participant) _(`POST /moderation/reports`; flag read through `FeatureService`; channel permissions via `MessagesDeps::access`, membership re-checked in `sr-moderation::reports`.)_
- [x] **MOD-RPT-003** [BE] Server-side evidence snapshot at report time: the message, its attachments index and embeds, and ten messages before and after, stored immutable _(`sr-moderation::snapshot` (ten before and after, attachments index, embeds); evidence table refuses UPDATE and DELETE.)_
- [x] **MOD-RPT-004** [BE] Reporter privacy: the reported person never learns the reporter; guild moderators see the reporter only when the reporter ticked "moderators may see my name"; platform staff always see it and opening it is access-logged (D-209, D-125) _(`rules::reporter_visible`; events never carry the reporter; `moderation.reporter.read` access-log row. Tests: `crates/sr-moderation/tests/db.rs`.)_
- [x] **MOD-RPT-005** [BE] Rate limit of reports per account per day (platform setting `moderation.reports_per_day`) and duplicate rule (one open report per reporter and target) _(setting `moderation.reports_per_day` (default 20) and the one-open-report unique index.)_
- [x] **MOD-RPT-006** [BE] Reports never auto-action; a report on a message places a hold on it and its attachments (spec 3.2) _(a message report places a system hold (`created_by` NULL) through `holds::place_hold_in`; nothing else changes. Dismissal releases it and reopening places it again, D-328 (N-20).)_
- [x] **MOD-RPT-007** [BE] `GET /moderation/reports/mine` and status for the reporter: received, under review, closed, plus the outcome line, never the other party _(`GET /moderation/reports/mine`; the reporter sees state and outcome line, never the case. No Settings page for it yet.)_
- [x] **MOD-RPT-008** [BE] Routing per spec 3.4 and D-209: guild content to the guild's moderators, DMs and group DMs, users outside a guild, servers and `child_safety` / `illegal_content` to platform staff, library and cinema items to reviewers and `DELETE_CONTENT` holders _(`rules::route` (unit tests). Reviewers see the cases of their kind through the API; there is no reviewer UI yet.)_
- [x] **MOD-RPT-009** [BE] Guild moderators' view: `GET /moderation/guilds/{guild_id}/reports` (`MANAGE_MESSAGES`) and an escalate action that hands the case to platform staff _(`GET /moderation/guilds/{guild_id}/cases` and `POST /moderation/cases/{id}/escalate`; a server moderator is wearing the `Who::GuildMod` hat of `rules::pick_who`.)_
- [x] **MOD-RPT-010** [FE] Report dialog (category, note, "moderators may see my name", also block) replacing the "reporting is not available" dialog; targets: message, user, channel, library item, cinema item _(`moderation/ReportDialog.tsx` (replaces the "not available" dialog); "also block" calls the existing block route.)_
- [x] **MOD-RPT-011** [FE] Report entry for a server (server menu) _(`useGuildMenuCore().report`; the member list passes its server on a user report.)_
- [ ] **MOD-RPT-013** [FE] Reporter status list in Settings (`GET /moderation/reports/mine` exists; no page yet)
- [x] **MOD-RPT-012** [BE] Reported DM messages readable by case staff within the snapshot only, access-logged with the case id (D-214) _(`Moderation::get_case` logs a `private_read` row with reason `case <id>` before returning a DM snapshot; server moderators never get one.)_

## MOD-CASE — Cases (spec section 4)
- [x] **MOD-CASE-001** [DB] `moderation_cases`, `moderation_case_evidence` (immutable), `moderation_case_notes` and `moderation_case_events` (append-only); `moderation_holds.case_id` foreign key (D-212) _(migration 1090.)_
- [x] **MOD-CASE-002** [BE] Lifecycle `open` → `triage` → `investigating` → `actioned` or `dismissed` → `closed`; reopen within a window, then a new case linked to the old one _(`rules::can_transition`, `set_state`, `reopen_case` (window `moderation.case_reopen_days`, default 30).)_
- [x] **MOD-CASE-003** [BE] Duplicate reports on the same target attach to the open case _(`file_report` attaches to the open case on the same target and flips routing for an always-escalated category.)_
- [x] **MOD-CASE-004** [BE] Queue (`GET /moderation/cases`) by state, category, severity, assignee and guild; claim and release (assign) _(`GET /moderation/cases`, `GET /moderation/guilds/{id}/cases`, `PUT|DELETE /moderation/cases/{id}/assignee`.)_
- [x] **MOD-CASE-005** [BE] Case detail with reports, evidence, notes, events, holds and appeal; opening the reporter or a DM snapshot is access-logged _(`GET /moderation/cases/{id}`.)_
- [x] **MOD-CASE-006** [BE] Internal notes (append-only) and the timeline; every action writes a case event and the platform access log _(notes and events are append-only (trigger); platform staff changes write the access log in the same transaction.)_
- [x] **MOD-CASE-007** [BE] Staff-opened cases (no report) for a person or a thing _(`POST /moderation/cases` (API only; no staff form in Admin yet).)_
- [x] **MOD-CASE-008** [BE] Actions from a case: warn (system DM), platform ban with `category` and `reason_category`, record a guild action taken, dismiss, close _(warn, record an action taken elsewhere, ban from the case (admin ban plus `reason_category`), dismiss and close.)_
- [ ] **MOD-CASE-009** [BE] Actions from a case that need other services: delete or restore content, remove an attachment, time out, kick, guild ban, suspend an account _(Not built: these actions are performed with the server's own tools and recorded on the case with `record_action`; doing them from the case needs the chat services.)_
- [x] **MOD-CASE-010** [BE] System DM to the subject when an action is taken, never naming the reporter; serious bans may skip it (spec 4.5) _(warn and a non-serious ban DM the subject through `SubjectNotifier`; never the reporter.)_
- [x] **MOD-CASE-011** [FE] Admin > Moderation: the case queue with filters and claim _(`admin/moderation/CaseQueue.tsx`.)_
- [x] **MOD-CASE-012** [FE] Admin > Moderation: the case page (timeline, evidence, linked accounts, holds with release, notes, actions, appeal) _(`admin/moderation/CasePage.tsx`.)_
- [x] **MOD-CASE-013** [BE] Guild-scoped escalation to platform staff keeps the case, flips its routing and logs it _(`Moderation::escalate`; the server keeps a list row and loses the detail.)_

## MOD-HOLD — Holds (spec sections 2 and 4.3)
- [x] **MOD-HOLD-001** [BE] Hold API in `sr-moderation` that other steps call (takedown, DSR): place on a message (with its attachments), an attachment, a submission or a user's messages in a window; reason and case required _(`sr-moderation::holds`: `place_hold_in` / `release_hold_in` for callers with their own transaction, `place_hold` / `release_hold` with the staff check.)_
- [x] **MOD-HOLD-002** [BE] Placing and releasing are platform-staff only (`MANAGE_CASES`, D-213); release needs a reason; both write case events and the access log _(`require_staff(MANAGE_CASES)`; both directions write a case event and an access-log row. The system-placed report hold is released by dismissal, not by staff (N-20).)_
- [x] **MOD-HOLD-003** [BE] Release of an old row makes it purgeable on the next job run and the job logs the count _(`release_hold_in` gives a message deleted while held a `purge_after`; the release logs the purgeable count. Test: `sr-server` `jobs::moderation_release_tests`.)_
- [x] **MOD-HOLD-004** [BE] Holds list with age, size of held content and the review flag after `moderation.hold_review_days` (default 90, D-212) _(`GET /moderation/holds` with `review_due` (`moderation.hold_review_days`, default 90) and `POST /moderation/holds/{id}/reviewed`.)_
- [x] **MOD-HOLD-005** [TEST] Retention guard tests: a held message, attachment and submission survive every purge job; a guard test for any new sweep (`XC-MOD-001` rule) _(new: held rejected submissions and held draft submissions skip the review housekeeping (`review_repo.rs`, test in `sr-db/tests/review_repo.rs`); release makes a held deleted message purgeable (`sr-server` `jobs.rs`).)_
- [x] **MOD-HOLD-006** [FE] Hold and release in the case page; held content badge for staff _(release and place in the case page and the Holds tab.)_

## MOD-APL — Appeals (spec section 4.6, D-215)
- [x] **MOD-APL-001** [DB][BE] One appeal per decision (a platform ban); reviewed by a different staff member, the Owner when nobody else holds the bit; no second appeal _(migration 1090 `moderation_appeals` (unique ban id); `rules::appeal_reviewer_ok`.)_
- [x] **MOD-APL-002** [BE] `POST /moderation/appeals` from the ban screen with the account's credentials (a banned account has no session), rate limited _(`POST /moderation/appeals` (public; `AuthService::appeal_account` shares the sign-in throttles).)_
- [x] **MOD-APL-003** [BE] Staff decision on an appeal (`uphold` or `overturn`, reason required); overturning lifts the ban through the existing unban path _(`POST /moderation/appeals/{id}/decision`; overturn calls the admin unban first.)_
- [x] **MOD-APL-004** [FE] The ban screen offers the appeal form after a failed login with a banned account _(`routes/entrance/BanAppeal.tsx` on the sign-in page.)_
- [x] **MOD-APL-005** [FE] Appeal panel in the case page _(appeal panel in the case page and the Appeals tab.)_

## MOD-TKD — Takedown register (spec section 5; D-293; step 2)
- [ ] **MOD-TKD-001** [DB] `takedown_notices` with notifier name and contact, claimed work or legal basis, exact URLs or ids, date received, original text, who entered it, state, due date, kind (`copyright` or `illegal_content`)
- [ ] **MOD-TKD-002** [BE] Intake in Admin > Moderation > Takedowns (staff-entered only, `MANAGE_TAKEDOWNS`); a case opens with `illegal_content` or a copyright category
- [ ] **MOD-TKD-003** [BE] Triage: identify the content, reject an incomplete notice with a templated reply
- [ ] **MOD-TKD-004** [BE] Hold, then disable: hold through the `MOD-HOLD` API, unpublish a library or cinema item, set a message to `staff_removed`, block the stored object on the media path
- [ ] **MOD-TKD-005** [BE] Notify the uploader or author by system DM with the notice summary and the counter-notice path
- [ ] **MOD-TKD-006** [BE] Counter-notice recorded by staff with `restore_after` = 10 business days (operator country's weekdays, no holiday calendar); the housekeeping job restores unless `court_filing_noted` is set; restore is a logged action
- [ ] **MOD-TKD-007** [BE] Strike counter: upheld takedowns per Person within `moderation.strike_window_months`; a review case opens at `moderation.strike_review_threshold`, never a suspension (D-293)
- [ ] **MOD-TKD-008** [BE] Access-log row for every state change of a notice; closed notices kept forever (D-212)
- [ ] **MOD-TKD-009** [BE] Register export (CSV and JSON)
- [ ] **MOD-TKD-010** [FE] Admin > Moderation > Takedowns: register, intake form, notice page with counter-notice and restore
- [ ] **MOD-TKD-011** [FE] Library reader and cinema player pages show that the item is held or taken down

## MOD-DSR — Data-subject requests (spec section 6; D-295, D-124; step 3)
- [ ] **MOD-DSR-001** [DB] `data_requests` (kind, requester, subject Person, verified by, state, `received_at`, `due_at`, `extended_until`, outcome)
- [ ] **MOD-DSR-002** [BE] `due_at` from `compliance.data_request_deadline_days` at creation; one documented extension with a reason (`POST /admin/data-requests/{id}/extend {days, reason}`), access-logged
- [ ] **MOD-DSR-003** [BE] Export run reusing the data-export job and README (G-66), triggerable by staff after identity verification, logged
- [ ] **MOD-DSR-004** [BE] Erasure run reusing account deletion, naming the retained records (Person record, invite graph, invoices, moderation history; D-124)
- [ ] **MOD-DSR-005** [BE] A deletion request that meets a hold keeps the held rows, states why, and flags the hold's case for a decision
- [ ] **MOD-DSR-006** [BE] Restriction or objection: staff tool to hide an account's activity from discovery while a request is reviewed, nothing deleted
- [ ] **MOD-DSR-007** [BE] Rectification by staff: correct a Person link with a logged reason
- [ ] **MOD-DSR-008** [FE] Admin > Moderation > Data requests queue with due dates and overdue marking

## MOD-AGE — NSFW and age policy enforcement (spec section 7)
- [ ] **MOD-AGE-001** [BE][FE] Mark and unmark series, titles and channels as age-restricted from a case or the item page, logged
- [ ] **MOD-AGE-002** [BE] Misplaced-NSFW report category with actions: move or delete the message, mark the channel `nsfw`, warn the author
- [ ] **MOD-AGE-003** [BE] Underage handling: reset the age confirmation; under 18 reset-and-restrict, under 13 suspended with `reason_category = other` (D-294)
- [ ] **MOD-AGE-004** [BE] Unfurls and embeds from age-restricted channels are not shown where the age gate does not apply

## MOD-BAN — Ban reason taxonomy (spec section 8; D-211)
- [x] **MOD-BAN-001** [DB] `ban_reason_category` enum and `person_bans.reason_category` _(migration 0580, XC-MOD-001.)_
- [ ] **MOD-BAN-002** [FE] Ban dialog picks `reason_category` with the serious-by-default mapping (D-211) and lets staff raise `harassment` and `ban_evasion` _(The ban dialog on a case picks `reason_category` with the D-211 presets; the Accounts and Bans pages still use the severity-only dialog.)_
- [ ] **MOD-BAN-003** [FE] Bans list filters by `reason_category`, severity and age

## MOD-DASH — Staff dashboards (spec section 9; step 4, later)
- [x] **MOD-DASH-001** [FE] Case queue (see `MOD-CASE-011`) _(see MOD-CASE-011.)_
- [ ] **MOD-DASH-002** [FE][BE] Reports inbox: unattached reports, duplicate grouping, bulk dismiss with a reason _(later)_
- [x] **MOD-DASH-003** [FE] Case detail (see `MOD-CASE-012`) _(see MOD-CASE-012.)_
- [x] **MOD-DASH-004** [FE][BE] Holds list with the review warning (API in `MOD-HOLD-004`; the dashboard view is later) _(the Holds tab shows the review flag and can mark a hold reviewed.)_
- [ ] **MOD-DASH-005** [FE] Takedown register view (see `MOD-TKD-010`)
- [ ] **MOD-DASH-006** [FE] Data requests view (see `MOD-DSR-008`)
- [ ] **MOD-DASH-007** [FE] Bans by reason, severity and age (see `MOD-BAN-003`)
- [ ] **MOD-DASH-008** [FE][BE] Health: reports per day, median time to first action, reopen rate, appeal outcomes; aggregates only, no per-staff ranking _(later)_
- [ ] **MOD-DASH-009** [FE][BE] Retention check: counts of rows skipped by purge jobs because of holds _(later)_
- [ ] **MOD-DASH-010** [FE][BE] Age and child-safety metrics (spec 7) _(later)_

## MOD-SUS — Inviter suspicion tooling (spec section 10; step 5, later)
- [ ] **MOD-SUS-001** [FE][BE] Suspicion panel on the Person page: `suspicion_events`, banned descendants with `reason_category`, invite path to the root _(later)_
- [ ] **MOD-SUS-002** [BE] Auto-opened case when a freeze or escalation threshold is crossed, assigned to senior staff and linked to the banning case _(later)_
- [ ] **MOD-SUS-003** [BE] Evasion signals as review items, never auto-acted: shared login IPs in the 180-day window, matching Discord identity, shared invite lineage, a new account soon after a ban _(later)_
- [ ] **MOD-SUS-004** [BE][FE] Clear with reason, logged and shown on the Person page; events are not deleted _(later)_
- [ ] **MOD-SUS-005** [BE] The inviter is told only that invite creation is paused for review _(later)_
