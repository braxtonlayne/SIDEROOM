# Handoff: feat/p39-ops-invites (wave 39, OPS-INV-001)

Branch `feat/p39-ops-invites`, base `integration/phase39` (28bdcbf9). Migrations used: **1890** only (1890 to 1899 reserved). Job id **1890**. Nothing pushed, no PR, nothing merged.

## Done (OPS-INV-001, all four bullets)

**Lock the Doors (`invites.locked`, boolean, default off, O-6).**
- Registration is refused in three places: the route (`POST /sr/register`, before any validation or hashing, error `900005` "The doors are closed right now."), the store (`RegisterRejection::Locked`) and inside the registration transaction itself (`sr_db::registration::register_with_invite` reads the setting first thing and returns `Rejection::Locked`), so even a valid, open invite registers nobody and nothing is written. Emergency invites are not exempt; the operator lifts the lock.
- `GET /sr/invites/{code}` answers `{valid:false, reason:"locked"}` for every code (no oracle). `GET /sr/compliance/public` gains `doors_closed`.
- Existing members are unaffected (login, sessions, their own invites screen and code creation keep working; asserted in the Postgres test).
- Audit: `PUT /admin/invites/lock {locked, reason}` writes exactly one `AdminAction` row, action `invites.lock` or `invites.unlock`, with the required reason, in the same transaction as the setting. A change through the generic `PATCH /admin/settings` is also audited as a lock (extra row after the `settings.patch` row). Setting the value it already has writes nothing.
- Entrance (D-385: the only entrance change): a "doors are closed" notice on the landing page (from `doors_closed`) and on the register page, the landing check message for `reason: locked`, and the register error for `900005`. No new entrance route, no app code before sign-in. Key `entrance.doors.closed` is in `en/entrance.ts` (first-paint core), the rest of the strings are in `en/p39-ops-invites-1.ts`.

**Quotas.**
- `invites.default_credits` (0..1000, default 5): the catalog row `invites.initial_credits` existed but registration never read it (it used `AuthConfig.initial_credits`). It is renamed (migration 1890 moves a saved value; the old key is gone from the catalog) and registration now reads it per request; the configured seed is the fallback. `invite_initial_credits` on the public compliance object follows it.
- `invites.max_outstanding` (1..1000, default 20): most unused codes per account, enforced under the issuer row lock in `member_invites::create` (new `InviteBlock::AtOutstandingMax`, reason `at_max`, shown in User Settings > Invites). The credit balance still caps first. Lowering it keeps existing codes.
- `invites.unused_code_days` (1..3650, default 30).

**Job `invites.prune`** (`crates/sr-server/src/invites_jobs.rs`, singleton 1890, every six hours, registered in `app.rs`).
- One transaction: revokes `open`, non-emergency codes older than `invites.unused_code_days`; marks (table `invite_inactive_roots`) every active account that redeemed an invite more than that many days ago and **never activated** (email never verified AND `last_seen_at` null) with the size of its subtree (recursive CTE over redeemed invites); lifts marks whose root activated; writes `invite_prune_runs`.
- It NEVER deletes `invite_graph` rows: `invite_graph` is a view over redeemed `platform_invites`, the pass only updates `open` ones, and it counts the view before and after and rolls back with an error if the numbers differ. The new tables hold display data only (no FK to accounts, nothing in them is read by the Person model or the suspicion review).

**Admin > Invites** (`InviteControlsCard.tsx`): Lock the Doors toggle with a reason dialog (needs `MANAGE_FEATURES`; others see a note), the three quotas (edited in Admin > Limits, the catalog rows), the last cleanup pass, the inactive subtrees list. New routes (all in `route_matrix.tsv`; the authz matrix reads that file): `GET /admin/invites/controls` (MANAGE_INVITES), `PUT /admin/invites/lock` (MANAGE_FEATURES), `GET /admin/invites/inactive` (MANAGE_INVITES).

## docs/09 unchanged
`docs/09-retention-and-privacy.md` is **not touched** by this branch (`git diff integration/phase39 -- docs/09-retention-and-privacy.md` is empty). The invite graph is kept forever; the prune job revokes codes and writes display marks only.

## Tests (see the end of this file for the final run results)
RESULTS_PLACEHOLDER

## Not done / open
- **Playwright not run** (8080/5173 belong to another agent). `e2e/tests/doors-closed.spec.ts` is written (stubbed API, three cases) and typechecks (`tsc --noEmit` in `e2e`). It still needs its first run by whoever owns the ports.
- Manual check on 8087 was not done.
- `docs/inventory/21-feature-phase.md`, `deploy/OPERATOR.md` (settings table, job list), `docs/06` (still says config `invites.initial_credits`) and decision D-372 (names `invites.initial_credits`) were not edited: they belong to the wave 40 docs agent. They need the rename `invites.initial_credits` -> `invites.default_credits` and a line for the three new settings, the lock and the job.
- The "Features" screen of the roadmap line: the new settings are catalog rows and so appear in Admin > Limits (invites group); `invites.locked` is deliberately a catalog row but its audited control is on Admin > Invites. Nothing was added to the feature registry (the lock is a setting, not a module), so ops-modules' registry walk is unaffected.

## Decisions to confirm
- "Never activated" = email never verified and never seen online by the gateway. Conservative on purpose (marks fewer accounts).
- The lock does not exempt operator emergency invites. The CLI creates accounts directly (`accounts::create_account`, not `register_with_invite`), so the operator tools are unaffected by it.
- Lock needs `MANAGE_FEATURES` (like every platform setting), viewing the controls needs `MANAGE_INVITES`.
