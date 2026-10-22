# Handoff: feat/p19-mod-age (MOD-AGE-001 to -004, MOD-DSR-005 to -007)

Base: integration/phase19 (= main). Migration used: `1160_mod_age_dsr.sql` (range 1160-1169). No new job ids. No new audit action ids (the channel change reuses the existing channel-update row). No new dependencies.

## Done
- **MOD-AGE-001** Mark and unmark a channel (`channels.nsfw`), a library series or a cinema title (`age_restricted`) as age-restricted, with a required reason, logged (`moderation.age.restrict` / `moderation.age.unrestrict`), from a case (button "Age restriction" on the case page, timeline event, a case being worked moves to `actioned`) or with no case (`POST /moderation/age/restrictions {kind, id, restricted, reason, case_id?}`). Allowed with `MANAGE_CASES` or `DELETE_CONTENT`, and with `REVIEW_READING` (series) or `REVIEW_MEDIA` (title). A channel change tells live clients (`CHANNEL_UPDATE`) and writes a guild audit row (existing channel-update action, change `nsfw`, reason "Platform staff: age restriction"). The item pages keep using the existing catalog editors (their age toggles); the new route is the logged way for a case and for any later page button.
- **MOD-AGE-002** The report category `nsfw_policy` already existed (migrations 0580 and 1090, report dialog). The case actions cover it: delete the message (existing), mark the channel (MOD-AGE-001), warn the author (existing). "Move" is not built (see decisions).
- **MOD-AGE-003** `POST /moderation/age/underage {account_id, band, note, case_id?}` (`MANAGE_CASES`). `under_18`: age confirmation reset and locked (`accounts.age_locked_at`, `POST /sr/age-gate` ignores "yes" while locked and answers 403 with a message, "no" is still recorded), the account gets a system DM. `under_13` (below the minimum registration age): the same, and the account is suspended first through the admin service (`MANAGE_ACCOUNTS`, audit reason starts "other:"), the inviter gets a DM saying only that the invitee did not meet the entry rules. `POST /moderation/age/underage/{id}/lift` removes the lock. Everything is access-logged (`moderation.age.underage`, `moderation.age.unlock`) and, with a case, on its timeline.
- **MOD-AGE-004** `sr_chat::sensitive::embeds_stay_behind`: crosspost (followed announcement channels) and forwards drop embeds that come from an age-restricted channel into a channel without the gate.
- **MOD-DSR-005** An erasure run that meets a hold keeps the rows (as before), flags each hold that still keeps a row of the account (`moderation_holds.erasure_*`), adds an `erasure_hold_flagged` event to the hold's case, and the outcome names the held rows, the flagged holds and the cases. Staff decide on the Holds tab: release (existing) or `POST /moderation/holds/{id}/retain {basis}` (legal basis required, `erasure_hold_retained` event, access log). The Holds tab shows a "Deletion request" badge and a Retain button.
- **MOD-DSR-006** `POST /admin/data-requests/{id}/restriction {hide, reason}` (`MANAGE_TAKEDOWNS`). `activity_shares()` (migration 0797, read by every cinema and library discovery, presence, aggregate and recommendation query) answers false while an active request has hidden activity, so it ends when the request is completed or declined, or when staff show it again. Queue: Hide/Show activity button, "Activity hidden" badge and reason. Nothing is deleted.
- **MOD-DSR-007** `POST /admin/data-requests/{id}/rectify {person_id?, reason}`: put the requester's account under a Person, or split it off. Needs `MANAGE_TAKEDOWNS` and `MANAGE_ACCOUNTS`; log rows `person.link` / `person.unlink` and `data_request.rectify`.
- **Follow-up M-06** (docs/dev/answers-followups-p16.md) is now fully covered and ticked with a note: p17 did the settings and copy, P18 the data-request due date and extension, the strike counter and the counter-notice restore job, P19 the age wiring. Only the Setup Wizard clause is not built (dropped by D-315).
- Docs: inventory `19-moderation.md` ticked; spec `moderation-compliance.md` section 17.

## Still open
- Item-page buttons for staff on the Library series page and the Cinema title page (the admin catalog editors already carry the age toggle; the route is ready).
- "Move a message" for a misplaced-NSFW report (no Discord counterpart; delete plus warning is the default).
- Search indexes of the library and cinema carry the age flag until their next rebuild (boot); the gate itself reads the column at once.
- Hiding an account from the member directory and from message search while a request is reviewed (only shared activity is hidden); no TV reader of `activity_shares` exists yet.
- Not built on purpose: mandatory external reporting (NCMEC and similar), waits on the owner's lawyer (D-336 h). No "needs legal escalation" flag: the spec does not suggest one.

## Decisions taken (conservative defaults; all for the owner's lawyer to check)
1. Marking is platform staff only (plus the review bits per kind); server moderators keep using their channel settings and record `mark_nsfw`.
2. An under-18 account stays active but cannot confirm 18+ itself until staff lift the lock; an account below the minimum age is suspended and the age is never told to the inviter. The inviter DM names the invitee's username.
3. The `under_13` band is "below the minimum registration age" (`accounts.minimum_age`), not a literal 13.
4. A restriction ends with the request (no separate expiry); staff can end it earlier.
5. Rectification needs both the takedown and the account permission, because it changes the Person graph used for ban evasion.
6. Retaining held rows after an erasure needs a stated basis and is a separate click from releasing.

## Tests run
- Postgres (scratch DBs via `TestDb`, dropped by the harness): `cargo test -p sr-moderation --test age`, `-p sr-db --test data_requests` (new: hold flag, restriction, rectification, age lock), `-p sr-api moderation::tests` (new: staff-only matrix rows, age tools over HTTP). See the final report for results.
- Unit: `embeds_stay_behind`. Web: `admin/__tests__/p19modage.test.tsx`.

## Open owner questions
- Is "suspend, reset and lock" the right handling below the minimum age, and should an under-18 account be told by DM (D-294 lawyer note)?
- Should rectification of a Person link also be open to `MANAGE_ACCOUNTS` alone (it already is, on the Persons page)?
- Is hiding shared activity enough for GDPR articles 18 and 21, or must the account also leave the member directory and message search?

## Found and fixed
- `link_in` (Person linking, `admin_repo_ext.rs`) deleted an emptied Person and failed with a foreign-key error once moderation cases, data requests or takedown rows pointed at it (since P18). It now re-points those rows to the target Person first.

## Gate results
fmt and clippy -D warnings clean on sr-moderation, sr-admin, sr-db, sr-api, sr-chat, sr-server (all targets); tests pass for those crates incl. the full sr-db suite with Postgres; web tsc, eslint, prettier, full vitest (274 files, 2469 tests), build and bundle budget pass. OpenAPI, api-client types and MSW mocks regenerated; route matrix rows added (`staff`); `KNOWN_CLASHES` empty. The forward path of MOD-AGE-004 has only the unit test of the rule (the crosspost path has a route test).

## Database note
Test runs used the maintenance database `postgres` as the `SR_TEST_DATABASE_URL` connection database; the coordinator was told what was left behind (migrated `postgres` database, template `sr_tpl_152_1160_e670414a`). Nothing was dropped.
