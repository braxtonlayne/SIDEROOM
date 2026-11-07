# fix/p55-staff (wave 55: staff step-up, D-574)

Base: `integration/phase54` `1480498f`. No migrations, no new dependencies, no new routes.

## Done
- **SEC-AUD-042 (D-574).** One rule, enforced by the existing route-layer gate (`reauth_gate`): every staff route
  that places, changes or ends a member restriction, a suspension or a perk is a `SENSITIVE_ACTIONS` row
  (`crates/sr-api/src/sensitive.rs`). New rows (15):
  - Member restriction: `POST /admin/moderation/members/{account_id}/{warn,timeout,invite-freeze,revoke-invites}`,
    `POST /admin/moderation/restrictions/{restriction_id}/lift` (a lift is an unfreeze; not named in the D-row, but
    it ends a member restriction, see question 1), `POST /admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/{freeze,clear}`,
    `POST /admin/moderation/restrictions/{restriction_id}/appeal-decision`, `POST /moderation/appeals/{appeal_id}/decision`.
  - Account suspension: `POST /admin/accounts/{account_id}/{suspend,unsuspend}`.
  - Age tools: `POST /moderation/age/underage`, `POST /moderation/age/underage/{account_id}/lift`,
    `POST /moderation/age/restrictions`.
  - Perk grant: `DELETE /admin/donations/entitlements/{grant_id}` (next to the grant, gated in wave 54).
  - Self-grant: `POST /admin/donations/entitlements` with `kind: patron` and the caller's own `user_id` answers
    `403 50001` ("You cannot grant a perk to your own account. Another staff member has to do it.") for every staff
    member, the Owner included, before any other validation; the refusal is in the access log as `denied`
    (`MoneyCtx::refuse_self_grant`). The window gate still runs first (a staff member without a fresh second factor
    meets `900004` first, D-362).
  - Web: every caller already went through `withReauth` (`admin/api.ts` suspend/unsuspend, `admin/donations/api.ts`
    revoke, `moderation/api.ts` appeal decision and age tools, `moderation/graduated.ts` warn/timeout/freeze/revoke/
    lift/appeal/node freeze and clear); nothing was missing. The admin grant dialog (Admin > Entitlements > Grant)
    now reads `/admin/me` and, when the Patron account id is the staff member's own, shows the refusal under the
    field and keeps Grant disabled (new string in `p55-staff-1.ts`).
  - Doc 07 section 6: the rule in prose and the table regenerated from the list (51 rows).
  - No change to `route_matrix.tsv` (every route was already a `staff` row) or the authz matrix (it counts `900004`
    as a step-up for an allowed caller and any 403 as a refusal).
  - e2e, because the `playwright` job keeps the window on and the bootstrapped Owner has no second factor:
    `moderation-gate.spec.ts` has a second Senior Staff reviewer R (TOTP) decide the appeal (the Owner did before;
    the access-log step now finds `moderation.appeal.decide` and `person.unban` under R; one more registered
    account); `admin-accounts.spec.ts` suspends and reactivates as a Senior Staff account with TOTP and answers the
    "Confirm it is you" dialog in the page (skipped when `SR_ADMIN_REQUIRE_REAUTH=0`).

## Tests
- Rust (fail without the change): `sensitive::tests::every_restriction_suspension_and_perk_write_is_gated` (reads the
  D-574 route families off `route_matrix.tsv`; a new write route there fails until listed), `reauth_list_tests`
  (walks the new rows closed, open, expired for the moderator and the Owner),
  `donations::tests::a_staff_member_never_grants_a_perk_to_their_own_account` (Treasurer and Owner, days and
  permanent, nothing granted; granting each other works).
- Vitest: `admin/__tests__/p55staffReauth.test.ts` (15 helpers: a 900004 opens the prompt once and retries once),
  `donations.test.tsx` new case (own id: message shown, Grant disabled, nothing posted; another id enables it).

RESULTS_PLACEHOLDER

## Owner questions (defaults chosen)
1. `POST /admin/moderation/restrictions/{id}/lift` is gated too (it ends a restriction early; the D-row's "lift" names
   the age tool). OK?
2. Not gated, as outside "member restriction, suspension or perk": `POST /moderation/cases/{id}/warn` (a system DM
   warning, no restriction record), `POST /admin/data-requests/{id}/restriction` (GDPR restriction of processing),
   `POST /admin/developers/apps/{id}/{suspend,unsuspend}` (an application, not a member),
   `POST /admin/moderation/subtree-cases/{id}/close` (closes the case, lifts nothing). The branch action, request
   approval and sterilization/branch requests already ask in the handler. Should any of these join the list?
3. Self-grant covers Patron grants to the caller's account only. A server-pool grant to a server the caller owns is
   still allowed (it pays the server's hosting share, not a perk for the person). Refuse that too?

## Threat-model row (proposed; the commander numbers it)
- Staff self-dealing and stale-session moderation: a hijacked staff session (no fresh second factor) could lift,
  overturn, suspend/unsuspend or revoke perks, and any staff member could grant themself Patron. Mitigation: every
  restriction/suspension/perk route needs the 15-minute second-factor window (`SENSITIVE_ACTIONS`, family test over
  the route matrix); a Patron grant to one's own account is 403 for everyone and logged as denied (D-574).
