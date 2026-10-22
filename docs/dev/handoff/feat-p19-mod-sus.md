# Handoff: feat/p19-mod-sus (MOD-SUS-001 to -005, spec section 10, step 5)

Base `integration/phase19` (= main). Migration range used: **1150** only (`1150_suspicion_review.sql`). Job id used: **1150** (`moderation.suspicion_housekeeping`; grep of `JOB_ID` constants showed it free). No numeric audit action ids were added: staff reads and actions go to the platform access log by name (`moderation.suspicion.view`, `moderation.evasion.list|detail|scan|resolve`).

## Done (all five items ticked in `docs/inventory/19-moderation.md`; notes in spec section 17)

- **MOD-SUS-001 panel.** `GET /moderation/persons/{id}/suspicion` (`MANAGE_INVITES`, logged): level and thresholds, history (`suspicion_events`, with the banned source account, the clear and its reason, the case an event opened), banned accounts below the Person (recursive invite tree, six levels, with severity, `reason_category`, lifted flag, newest case), invite path to the root per account, the automatic cases, and the evasion signals (only with `MANAGE_ACCOUNTS`). Web: `admin/sections/SuspicionPanel.tsx` replaces the old plain log in the Person drawer.
- **MOD-SUS-002 automatic case.** `AdminService::ban_person` now reports a threshold crossing (`SuspicionChange.freeze_crossed/escalate_crossed`, `banned_person_id`) to a `SuspicionObserver` (new `sr_admin::suspicion_hook`, a shared `ObserverSlot` filled in `sr-server/app.rs` because `Moderation` needs the admin service for its case actions). `Moderation::open_suspicion_case`: one automatic case per Person (`moderation_cases.auto_kind`, partial unique index), the escalation upgrades the freeze case, platform-routed, `serious`, linked to the newest case about the banned invitee, assigned to the senior-staff member with the fewest open cases, unassigned if there is none. Existing staff alerts (`InviterSuspicion`, `InviterEscalated`) are unchanged.
- **MOD-SUS-003 signals.** `evasion_signals` review items (shared login address in the 180-day window, shared invite lineage, new account soon after a ban). Pure rules in `suspicion_rules.rs` with unit tests. Routes: `POST /moderation/persons/{id}/evasion-scan`, `GET /moderation/evasion-signals`, `GET /moderation/evasion-signals/{id}`, `POST /moderation/evasion-signals/{id}/review`. Job `moderation.suspicion_housekeeping` (daily): scans people who signed in recently and deletes `login_events` older than 180 days. Web: Signals tab in Admin > Moderation (`MANAGE_ACCOUNTS`), signal rows in the panel. **Nothing acts on a signal.**
- **MOD-SUS-004 clear.** The existing clear route (reason required, access-logged) now stores the reason in the suspicion event, the panel shows cleared steps, and the open automatic case gets a `suspicion_cleared` timeline entry. Events are never deleted.
- **MOD-SUS-005 inviter notice.** Invites screen says "Invite creation is paused for review." (was "unavailable for this account right now"); one system DM at the freeze crossing with the same words (`suspicion_rules::INVITER_NOTICE`, unit-tested to name no cause). Redemption of a paused inviter's code was already the generic "invite unavailable".

## Privacy

- IP-derived data is staff only (`MANAGE_ACCOUNTS`), every read of it is access-logged, and the panel hides the signals from staff without that bit (the server withholds them, not just the client).
- A signal row stores counts and words, never an address. Addresses are read live from `login_events` on the logged detail call, so the 180-day window and the phase-18 account-deletion scrubbing (`login_events.ip` and session addresses blanked) apply by construction. Tests: old history, shared networks (five people on one address) and blanked addresses raise nothing; the stored detail and the list contain no address.
- Addresses used by more than four Persons count as shared networks and raise nothing (conservative default, `MAX_PERSONS_PER_SHARED_IP`).
- Never auto-ban: tests assert no `person_bans` row, no state change on the newcomer after a scan; review (`dismissed`/`confirmed`) changes only the signal.

## Follow-ups P-10 to P-16 (D-344 to D-348), done in the same branch

- **P-10** the clear sends "Invite creation is available again." once, only when it ends the pause and no hand freeze remains (`SuspicionObserver::cleared` gained `was_paused`).
- **P-11** `moderation.shared_ip_max_persons` (4, 2 to 50) and `moderation.new_account_after_ban_days` (14, 1 to 90) are platform settings read by the scan and the detail; Admin > Limits renders them from the catalog (the page is generic, so its existing bounds tests cover them).
- **P-12** migration **1151** (`detail_snapshot`, `case_id`, evidence kind `evasion_signal`, `suspicion_events.kind`); a confirmed review snapshots and writes case evidence; the detail route returns the snapshot after the 180-day purge; the purge runs in 20,000-row slices, 25 per run, and the job comes back after five minutes while a backlog remains. Doc 09 and spec section 2 updated.
- **P-13** refused re-registration writes a `reregistration_refused` event on the banned Person and the existing alert names the inviter's Person.
- **P-14** link-case route, "Open case" button (prefilled dialog), case link shown on the signal row and on the case timeline. The case page lists no signal (that would show IP-derived data to `MANAGE_CASES` holders).
- **P-15** `GET /moderation/evasion-signals/count` and the Review hub tile. The Reports count was not added.
- **P-16** Moderation visible to `MANAGE_ACCOUNTS`; accounts-only viewers see the Signals tab alone.

Decision 3 below is superseded by D-346 (settings). Migration range used: 1150 and 1151.

## Still open / not done

- **Matching Discord identity** as a signal: D-030 makes the id unique across all accounts including deleted ones, so a reused id is refused when linked; there is no second stored account to compare. Not built; item 003 ticked for the other three kinds.
- One-click "open a case from a signal", and a signals count in the Review hub (optional, not requested).
- The Signals tab is inside Admin > Moderation, which a staff member with `MANAGE_ACCOUNTS` but neither `MANAGE_CASES` nor `MANAGE_TAKEDOWNS` sees only as the case queue (they still reach signals from the Person page).
- Not exercised in a browser (no Playwright, no servers on 8080/5173 in this wave).

## Decisions taken (conservative defaults; owner may change)

1. **Senior staff** = accounts whose roles together hold both `MANAGE_CASES` and `MANAGE_TAKEDOWNS` (D-210 gives Senior Staff both), not a role name, so a renamed role still works. Least-loaded wins; none means unassigned.
2. **Notice to the inviter** also goes as a system DM at the freeze crossing (not at escalation). If the owner prefers the Invites screen text only, drop the `dm_subject` call in `open_suspicion_case`.
3. **Corroboration:** "same inviter as a banned person" and "created within 14 days after a ban" alone raise nothing (innocent people share inviters and sign up after bans); invited-by-a-banned-person stands alone.
4. **`login_events` retention was not enforced anywhere** although doc 09 promises 180 days; the new daily job deletes older rows. Say so if a longer window is wanted.
5. The automatic case uses category `other`, severity `serious`, `opened_by_kind = staff` with the banning staff member as `opened_by_id` and no actor on its timeline events.

## Tests run

Results (all green): sr-admin 60; sr-moderation 34 unit + 14 + 7 + 16 Postgres; sr-api 649 lib (incl. 14 moderation route tests), route_matrix 5, openapi_names 2; sr-db admin_repo 10, member_invites 8, p13_settings_admin 2, data_requests 6, registration 4; sr-server 125; fmt and clippy -D warnings clean; web typecheck, eslint, prettier, full vitest 274 files / 2467 tests, vite build, bundle budget (admin 357.7 of 600 KB gzip). Commands:
- `cargo fmt --check`; `cargo clippy -j 2 -p sr-admin -p sr-db -p sr-moderation -p sr-api -p sr-server --all-targets -- -D warnings`.
- `cargo test -p sr-moderation` (unit 34 incl. 8 new in `suspicion_rules`) and `--test suspicion -- --include-ignored --test-threads=1` (7 Postgres tests: auto case, no-senior, panel, shared address and privacy, shared networks/old history/scrubbed, lineage, end to end through `AdminService`).
- `cargo test -p sr-api` moderation route tests (denials by name and logged, bad input, Postgres flow over HTTP), `route_matrix`, `openapi_names`.
- Web: typecheck, eslint, prettier, `p19suspicion.test.tsx` (3 files: panel, signals, tab), `p13people.test.tsx` updated, full vitest once, build and bundle budget.
- Scratch Postgres: `TestDb` per-test databases (`sr_t_*`, dropped by each test) and the `sr_tpl_*` template databases it leaves behind (dropped at the end).

## Owner questions

Answered 2026-10-22 by the owner's proxy (delegated): D-344 to D-348 in `docs/01-decisions-log.md`, P19-SUS-1 to -5 in `docs/open-question-answers.md`, follow-ups P-10 to P-16 in `docs/dev/answers-followups-p16.md`. In short: decision 1 confirmed; the DM stays and a mirror DM goes at the clear; the cut-off and the window become settings, the 180 days stay a constant; the deletion is the intended enforcement with no hold exception (a confirmed signal keeps a no-address snapshot); the Discord gap is closed by D-030 with a visibility item, the other three gaps are scheduled.

1. Is a system DM to the inviter at the freeze welcome, or should only the Invites screen say it (decision 2)?
2. Should the shared-network cut-off (four people per address) and the 14-day "soon after a ban" window be settings? They are constants now.
3. Is deleting `login_events` after 180 days (decision 4) the intended enforcement of doc 09?
