# 19 — Moderation & compliance (`MOD`)

Phase 7. Spec: [moderation-compliance.md](../specs/moderation-compliance.md). Decisions: D-122, D-125, D-209 to D-216, D-293 to D-295, D-315. Owner round: M-25 in [answers-followups-p16.md](../dev/answers-followups-p16.md). Reserved hooks that already exist are `XC-MOD-001` to `XC-MOD-003` in [18-cross-cutting.md](18-cross-cutting.md).

Delivery order (spec section 13, D-315): step 1 = `MOD-RPT`, `MOD-CASE`, `MOD-HOLD`, `MOD-APL`, `MOD-PERM`; step 2 = `MOD-TKD`; step 3 = `MOD-DSR`; step 4 = `MOD-DASH` (later); step 5 = `MOD-SUS` (later). `MOD-AGE` and `MOD-BAN` are spec sections 7 and 8, listed so nothing in the spec is missing; they have no step yet. Each step has its own branch and flag.

## MOD-PERM — Permissions (spec section 11)
- [x] **MOD-PERM-001** [DB][BE] Platform bits `MANAGE_CASES` (22) and `MANAGE_TAKEDOWNS` (23); Moderator gains `MANAGE_CASES`, Senior Staff both (D-210) _(migration 0793, `sr-core` platform_permissions, `sr-admin` rules; built before this inventory.)_
- [ ] **MOD-PERM-002** [BE] Every `/moderation` staff route checks its bit server-side and writes denials to the access log _(routes: `MANAGE_CASES`; takedown register and data requests: `MANAGE_TAKEDOWNS`.)_

## MOD-RPT — Reports (spec section 3)
- [ ] **MOD-RPT-001** [DB] `moderation_reports` (reporter, target kind and id, category, note, state, outcome line) with one open report per (reporter, target)
- [ ] **MOD-RPT-002** [BE] `POST /moderation/reports` behind the `moderation.reports` flag: category from the section 8 vocabulary plus `other`, note up to 2000 characters, optional "also block", visibility check on the target (a DM report only by a participant)
- [ ] **MOD-RPT-003** [BE] Server-side evidence snapshot at report time: the message, its attachments index and embeds, and ten messages before and after, stored immutable
- [ ] **MOD-RPT-004** [BE] Reporter privacy: the reported person never learns the reporter; guild moderators see the reporter only when the reporter ticked "moderators may see my name"; platform staff always see it and opening it is access-logged (D-209, D-125)
- [ ] **MOD-RPT-005** [BE] Rate limit of reports per account per day (platform setting `moderation.reports_per_day`) and duplicate rule (one open report per reporter and target)
- [ ] **MOD-RPT-006** [BE] Reports never auto-action; a report on a message places a hold on it and its attachments (spec 3.2)
- [ ] **MOD-RPT-007** [BE] `GET /moderation/reports/mine` and status for the reporter: received, under review, closed, plus the outcome line, never the other party
- [ ] **MOD-RPT-008** [BE] Routing per spec 3.4 and D-209: guild content to the guild's moderators, DMs and group DMs, users outside a guild, servers and `child_safety` / `illegal_content` to platform staff, library and cinema items to reviewers and `DELETE_CONTENT` holders
- [ ] **MOD-RPT-009** [BE] Guild moderators' view: `GET /moderation/guilds/{guild_id}/reports` (`MANAGE_MESSAGES`) and an escalate action that hands the case to platform staff
- [ ] **MOD-RPT-010** [FE] Report dialog (category, note, "moderators may see my name", also block) replacing the "reporting is not available" dialog; targets: message, user, channel, library item, cinema item
- [ ] **MOD-RPT-011** [FE] Report entry for a server (server menu); reporter status list in Settings
- [ ] **MOD-RPT-012** [BE] Reported DM messages readable by case staff within the snapshot only, access-logged with the case id (D-214)

## MOD-CASE — Cases (spec section 4)
- [ ] **MOD-CASE-001** [DB] `moderation_cases`, `moderation_case_evidence` (immutable), `moderation_case_notes` and `moderation_case_events` (append-only); `moderation_holds.case_id` foreign key (D-212)
- [ ] **MOD-CASE-002** [BE] Lifecycle `open` → `triage` → `investigating` → `actioned` or `dismissed` → `closed`; reopen within a window, then a new case linked to the old one
- [ ] **MOD-CASE-003** [BE] Duplicate reports on the same target attach to the open case
- [ ] **MOD-CASE-004** [BE] Queue (`GET /moderation/cases`) by state, category, severity, assignee and guild; claim and release (assign)
- [ ] **MOD-CASE-005** [BE] Case detail with reports, evidence, notes, events, holds and appeal; opening the reporter or a DM snapshot is access-logged
- [ ] **MOD-CASE-006** [BE] Internal notes (append-only) and the timeline; every action writes a case event and the platform access log
- [ ] **MOD-CASE-007** [BE] Staff-opened cases (no report) for a person or a thing
- [ ] **MOD-CASE-008** [BE] Actions from a case: warn (system DM), platform ban with `category` and `reason_category`, record a guild action taken, dismiss, close
- [ ] **MOD-CASE-009** [BE] Actions from a case that need other services: delete or restore content, remove an attachment, time out, kick, guild ban, suspend an account
- [ ] **MOD-CASE-010** [BE] System DM to the subject when an action is taken, never naming the reporter; serious bans may skip it (spec 4.5)
- [ ] **MOD-CASE-011** [FE] Admin > Moderation: the case queue with filters and claim
- [ ] **MOD-CASE-012** [FE] Admin > Moderation: the case page (timeline, evidence, linked accounts, holds with release, notes, actions, appeal)
- [ ] **MOD-CASE-013** [BE] Guild-scoped escalation to platform staff keeps the case, flips its routing and logs it

## MOD-HOLD — Holds (spec sections 2 and 4.3)
- [ ] **MOD-HOLD-001** [BE] Hold API in `sr-moderation` that other steps call (takedown, DSR): place on a message (with its attachments), an attachment, a submission or a user's messages in a window; reason and case required
- [ ] **MOD-HOLD-002** [BE] Placing and releasing are platform-staff only (`MANAGE_CASES`, D-213); release needs a reason; both write case events and the access log
- [ ] **MOD-HOLD-003** [BE] Release of an old row makes it purgeable on the next job run and the job logs the count
- [ ] **MOD-HOLD-004** [BE] Holds list with age, size of held content and the review flag after `moderation.hold_review_days` (default 90, D-212)
- [ ] **MOD-HOLD-005** [TEST] Retention guard tests: a held message, attachment and submission survive every purge job; a guard test for any new sweep (`XC-MOD-001` rule)
- [ ] **MOD-HOLD-006** [FE] Hold and release in the case page; held content badge for staff

## MOD-APL — Appeals (spec section 4.6, D-215)
- [ ] **MOD-APL-001** [DB][BE] One appeal per decision (a platform ban); reviewed by a different staff member, the Owner when nobody else holds the bit; no second appeal
- [ ] **MOD-APL-002** [BE] `POST /moderation/appeals` from the ban screen with the account's credentials (a banned account has no session), rate limited
- [ ] **MOD-APL-003** [BE] Staff decision on an appeal (`uphold` or `overturn`, reason required); overturning lifts the ban through the existing unban path
- [ ] **MOD-APL-004** [FE] The ban screen offers the appeal form after a failed login with a banned account
- [ ] **MOD-APL-005** [FE] Appeal panel in the case page

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
- [ ] **MOD-BAN-002** [FE] Ban dialog picks `reason_category` with the serious-by-default mapping (D-211) and lets staff raise `harassment` and `ban_evasion`
- [ ] **MOD-BAN-003** [FE] Bans list filters by `reason_category`, severity and age

## MOD-DASH — Staff dashboards (spec section 9; step 4, later)
- [ ] **MOD-DASH-001** [FE] Case queue (see `MOD-CASE-011`)
- [ ] **MOD-DASH-002** [FE][BE] Reports inbox: unattached reports, duplicate grouping, bulk dismiss with a reason _(later)_
- [ ] **MOD-DASH-003** [FE] Case detail (see `MOD-CASE-012`)
- [ ] **MOD-DASH-004** [FE][BE] Holds list with the review warning (API in `MOD-HOLD-004`; the dashboard view is later)
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
