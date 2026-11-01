# Integration of wave 44 (MOD-GRD-001, MOD-VCH-001): graduated moderation and profile vouches

Branch `integration/phase44`, base `origin/integration/phase43` (fast-forwarded to 09704378, PR #74's CI fixes
`50969e4d` and `09704378`), then `feat/p44-mod-tools` (86fc295a, MOD-GRD-001) and `feat/p44-heavy-crown` (8906d7a1,
MOD-VCH-001), both `--no-ff`, and at the end `origin/main` 16df52e5 (PR #74 merged; no content change). Not pushed; no PR opened.

Handoffs: [feat-p44-mod-tools](feat-p44-mod-tools.md), [feat-p44-heavy-crown](feat-p44-heavy-crown.md).
Decisions: D-420 to D-424 (44-A), D-425 to D-429 (44-B); D-422 amended by the integrator (the Operator, below).
Threat model: T43 (44-A, the real row; 44-B's placeholder dropped), T44 (44-B). Migrations 2120 (44-A), 2130 (44-B);
the integrator added none.

## Merge conflicts and how they were resolved

- `crates/sr-server/src/app.rs`: both kept. 44-B's `moderation.with_strike_weight(VouchStrikeWeight)` now runs
  **before** 44-A's `set_suspicion_observer` / `with_notice_sink` / `subscribe_serious_ban(moderation)` (as merged, the
  copy subscribed to the serious-ban event and the suspicion observer would have been taken before the multiplier was
  set, so the case's automatic freezes would never have seen it). 44-B's `subscribe_serious_ban(VouchFallout)` follows
  44-A's subscription.
- `crates/sr-moderation/src/service.rs`: the seam (`with_strike_weight`, `strike_weight_multiplier`) was in both
  (44-B cherry-picked 41a78d8c); one copy kept with 44-A's `notice_sink`.
- `docs/15-threat-model.md`: 44-A's T43, 44-B's T44 (placeholder T43 dropped); `scripts/threat-model.test.mjs` 5 ok.
- `docs/inventory/21-feature-phase.md`: one "Wave 44" header, MOD-GRD-001 then MOD-VCH-001 (44-B had ticked its item;
  unticked until the merged-tree proofs ran, then ticked).
- `docs/01-decisions-log.md`: D-420..D-424 then D-425..D-429.
- `route_matrix.tsv`, OpenAPI, api-client and MSW handlers merged clean; regenerated once at the end (1199 operations,
  `mocks:check` green; the only change was the branch-action description).

## What the integration changed

1. **Sterilized members cannot vouch.** `crates/sr-server/src/vouch_wiring.rs::SterilizationState` implements
   `sr_vouch::SterilizationCheck` over 44-A's state (`sr_moderation::is_sterilized`: an active, unexpired
   `member_restrictions` row of kind `sterilization`) and `app.rs` builds `Vouches` with
   `with_sterilization_check`. A database fault answers "sterilized" (fails closed: the vouch is refused, never given
   by someone who may not). Test: `sr-server` lib `vouch_pg_tests::a_sterilized_member_cannot_vouch` (active
   sterilization refuses `PUT` and `can_vouch`; expired, lifted and a plain invite freeze do not).
2. **The multiplier in 44-A's automated paths.** 44-A's engine has exactly one automated penalty: the automatic
   case freeze of a D-392-unactivated descendant. Its strike weight now carries `strike_weight_multiplier` (it was a
   fixed 1.0); its 30-day length is **not** scaled, on purpose: it ends in the account's removal, so a longer freeze
   would be the lighter outcome. The branch-action freeze records the multiplier as its weight too (like a per-node
   freeze); its 12-month length is D-395's. Staff acts (warn, timeout, per-node freeze) already read it.
   `automated_timeout_minutes` exists but **no automated timeout calls it in 44-A's engine**; the only automated
   timeouts in the product are AutoMod's, which 44-B already wraps (`WeightedMemberTimeouter`, `sr_vouch::tiers`).
   **No automated hold exists** (`sr-moderation`'s holds are evidence holds, not penalties), so `scale_hold` has no
   caller yet; nothing was invented. Test: `sr-moderation --test graduated
   the_automatic_case_freeze_weighs_by_the_multiplier` (weight 1.5, 30 days).
3. **The branch action is the platform Operator's** (roadmap: route-matrix actor `operator`; 44-A used `OWNER`). In
   this codebase "Operator" is the `OPERATOR` platform bit (doc 07 section 3: infrastructure, Owner-only to grant,
   `sr_admin::rules::OWNER_ONLY`; the Owner holds it implicitly), and the G-19 vocabulary expresses an actor as AUTHZ
   `staff` plus a PLATFORM cell. So the row is now `staff` / `OPERATOR`; `sr_moderation::is_operator` (Owner or the
   bit) guards the route, the proposal, the execution and the drill; a branch **ban** also needs `PLATFORM_BAN`
   (it bans each node through the platform ban). The approver still cannot execute. D-422 and T43 updated.
   Proof (`authz_matrix::graduated_moderation_holds_over_http`): the roster's Operators are exactly `staff_owner` and
   `staff_operator`; `staff_operator` without a step-up is stopped at 900004 (it passed the Operator gate); every other
   actor (all roster actors, a staff member with every bit but OWNER and OPERATOR, the second approver) is refused
   before the step-up; a delegated Operator (`STAFF_BASIC|OPERATOR` only) runs the approved freeze.
4. **Category-only notices, tested together.** The gate proof checks the voucher's private notice (one each, names
   the category, no handle of the tree, no account id, no staff note) and the MOTD (category and counts; no handle
   of the tree or the vouchers, no account id).
5. **Erasure.** Vouches given and received go (44-B, `account_deletion.rs`); `member_restrictions` and
   `moderation_subtree_nodes` stay as staff-side moderation history like every other moderation record (D-124, G-66);
   the case is referenced only in staff tables and the audit log (the 30-day removal queues an
   `account_delete_requests` row whose reason names no case). Test: `sr-db --test account_deletion
   erasure_removes_vouches_and_keeps_the_moderation_record`.
6. **One gate proof** on the merged tree: `crates/sr-server/tests/authz_matrix.rs::graduated_moderation_holds_over_http`
   (Postgres, real server wiring): 4-generation tree with one D-392 unactivated Gen-4 node and 3 profile vouchers
   (they `PUT /users/{id}/vouch` over HTTP on pages opened to members); a serious ban on Gen 3 through the staff route
   opens one case; only the unactivated Gen-4 node is frozen; Gen 1 and 2 untouched and still `POST /sr/invites` 201;
   the vouches for Gen 3 are removed (one for Gen 2 stays), each voucher got one category-only notice and has no
   restriction and is still active; the MOTD carries category and counts only; sterilization of Gen 2 fails for the
   proposer and without the approver's step-up and succeeds with both, +12 months; branch-action as in 3. The test
   now finds its case by root (the matrix seeds added a case of their own after 44-A ran it).
7. **Web:** `?tab=subtree` deep link on Admin > Moderation (for the pseudo-locale row); `i18n-pseudo.spec.ts` gained
   "admin subtree cases" and "profile vouch panel" (the Owner's own `/u/<handle>`) at desktop and phone.

## Gates

Scratch DBs `sr_p44_integ` (tests) and `sr_p44_e2e` (Playwright), both dropped at the end. `-j 2`, one cargo or test
process at a time.

| Gate | Result |
|---|---|
| `cargo fmt --all --check`; clippy `--all-targets -D warnings` (sr-admin, sr-moderation, sr-vouch, sr-api, sr-gateway, sr-server, sr-cli, sr-db) | ok |
| sr-db `secret_columns` / `migration_versions` / `account_deletion` / `data_export` | 4 / 1 / 5 / 2 pass |
| sr-admin lib; sr-vouch (unit + Postgres `vouches`); sr-moderation `--test graduated` | 74; 6 + 8; 9 pass |
| sr-api lib (`--include-ignored`) | 814 pass (first run: `routes::moderation::tests::evasion_signals_and_the_panel_over_http` failed once, passed alone and on the full rerun; untouched by wave 44, flaky) |
| sr-api `route_matrix` / `openapi_names` | 6 / 2 pass (`KNOWN_CLASHES` empty) |
| sr-gateway lib | 206 pass |
| sr-server lib (`--include-ignored`) / `web_gate` / `feature_registry` | 252 / 22 / 7 pass |
| sr-server `immutable_rules` (`--include-ignored`) | 25 pass |
| sr-server `immutable_rules` nightly, `SR_IMMUTABLE_STATES=normal,kill_features,kill_all` | groups 0, 2-5 clean in all three states; the admin group had 5 "status 0 timeout or connection failure" calls in `kill_all` (no real admission; the same run's table-change note listed job tables) and passed on the rerun `SR_IMMUTABLE_GROUPS=1`, normal + kill_all |
| sr-server `authz_matrix` (`--include-ignored`, all 11 incl. the gate proof) | 11 pass; no ceiling, `WRITE_OBSERVED` or length changed |
| OpenAPI, api-client types, MSW handlers regenerated once; `mocks:check` | 1199 operations, ok |
| `pnpm typecheck` (incl. e2e), `pnpm lint` (eslint + `prettier --check .`) | ok |
| vitest full `--maxWorkers=2` (wasm built first) | 440 files, 4018 tests pass |
| `node --test scripts/*.test.mjs`; `i18n-audit`; `check-css-tokens` | 100 pass (2 skipped); TOTAL 0; 69 files clean |
| `build-web.mjs`; `check-bundle-budget.mjs`; `check-entrance-isolation.mjs` | ok; ok; every public surface clean (profile 113.5/150 KB, guest-play 114.0/120 KB) |
| Playwright Chromium (dev server + sr-server on 8080, in-memory storage): exit-gate, moderation-gate, lounge-shell | 7 pass |
| Playwright Chromium: i18n-pseudo (all rows + "admin subtree cases" and "profile vouch panel", desktop and phone) | 30 pass (both new pages called their APIs, 200) |
| Playwright Chromium: a11y-signed-in, lounge-gaps (the specs that touch profiles) | 39 pass, 1 fail: axe `label-content-name-mismatch` on **library home** (`._link_` of the Library section bar, wave 41 `bed2fd5d`); no wave 44 file is involved, left for a follow-up |
| origin/main (16df52e5, PR #74) merged at the end | tree identical to the tested tree (`git diff HEAD~1 HEAD` empty), so no gate re-run was needed |

## Not done

- No automated **hold** and no automated timeout in 44-A's engine to scale: `sr_vouch::tiers::scale_hold` and
  `Moderation::automated_timeout_minutes` have no production caller (AutoMod timeouts are scaled by 44-B's decorator).
  Nothing was invented.
- The a11y-signed-in library-home finding above (pre-existing, Library section bar).
- `noindex.spec` not run (Caddy only); Firefox and WebKit not run locally.
- The CLI branch action still asks for no TOTP code (owner question).
- Signed-out `/~handle` (public profile surface, D-385) shows no vouch panel (44-B, unchanged).

## Owner items

Rows added to [owner-checklist.md](../owner-checklist.md) section 4b: CLI branch-action TOTP, who is the Operator,
D-421 7-day rule, the serious set, vouch removal by the recipient, vouch limits, the voucher notice naming, vouching
needs a visible profile.

## Resume here

Nothing half-done. Next: the owner's go-ahead to push `integration/phase44` and open the PR
"MOD-GRD-001, MOD-VCH-001: graduated moderation and Heavy Crown profile vouches (wave 44)". Optional follow-up: the
Library section bar's accessible name (a11y-signed-in, library home).
