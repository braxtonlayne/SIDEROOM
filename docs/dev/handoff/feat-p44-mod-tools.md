# Handoff: feat/p44-mod-tools (wave 44-A, MOD-GRD-001)

Branch `feat/p44-mod-tools` from `integration/phase43`. Not pushed. Decisions D-420 to D-424, threat-model row T43,
migration 2120 (`2120_graduated_moderation.sql`, job id 2120).

## Seam for 44-B (committed first, `41a78d8c`)

- **Serious ban event.** `sr_admin::SeriousBanObserver` (`crates/sr-admin/src/serious_ban.rs`). Subscribe with
  `AdminService::subscribe_serious_ban(Arc<dyn SeriousBanObserver>)`; called after a ban whose stored category is
  `serious` commits, with `SeriousBan { ban_id, person_id, source_account_id, actor_id, reason_category, note }`.
  `AdminService::ban_person_with_reason` carries the reason category; `ban_person` delegates with `None`.
- **Multiplier.** `sr_admin::StrikeWeight::strike_weight_multiplier(account) -> f32` (async trait, default
  `DefaultStrikeWeight` = 1.0). The integrator wires 44-B's impl with `Moderation::with_strike_weight(Arc<dyn StrikeWeight>)`;
  `Moderation::strike_weight_multiplier` never returns below 1.0. Read by warn/timeout (stored as `weight`) and
  `automated_timeout_minutes`. `sr_admin::scale_up` helper.
- Wiring in `crates/sr-server/src/app.rs`: `subscribe_serious_ban(moderation)` is already there; 44-B adds its own
  subscription next to it. Vouchers are notified, never penalised: nothing here touches them.

## Done

- Per-member acts (routes under `/admin/moderation/members/{id}/`: warn, timeout, invite-freeze, revoke-invites, ban
  with required `reason_category`, actions history), restriction lift and appeals (member side
  `GET /sr/moderation/restrictions`, `POST .../{id}/appeal`; staff `restriction-appeals`, `appeal-decision`).
- Serious-ban subtree case (`Moderation::open_subtree_case`): only D-392-unactivated descendants frozen (cannot post,
  invite, join voice; can sign in and appeal), 30-day removal sweep (job `moderation.restrictions`), everyone else and
  the two inviters listed; clear/freeze/ban per node, close.
- Sterilization and branch action: propose, second approver, 2FA step-up on both web calls; branch action Owner-only
  route and `sr-cli moderation branch-action`; hash-chained audit with case, approver, node count.
- Anonymised notice (platform banner, category and counts only), `GET /moderation/transparency`, Settings > Privacy
  cards, Admin > Moderation > Subtree cases UI, i18n fragment `p44-mod-tools-1.ts`, CLI parity rows.
- `sr-cli moderation drill --synthetic` (rolled back, report to Staff Server `#alerts` only).
- Enforcement: REST posting routes (`403 900124`), gateway voice join, platform invite creation; voice seconds persisted
  (`account_activity`) when a voice session ends.
- Docs: D-420..D-424, T43, inventory section header (MOD-GRD-001 left unticked for the integrator).

## Tests run

Scratch DB `sr_p44_mod_tools` (dropped at the end), `-j 2`, one cargo command at a time.

- `cargo test -p sr-admin --lib serious_ban` (5 ok).
- `cargo test -p sr-moderation --test graduated -- --include-ignored --test-threads=1` (8 ok): the gate proof at service
  level (4-generation tree, one unactivated Gen-4 node, case, only unactivated frozen, Gen 1-2 invite, notice text),
  sterilization (approver rules, +12 months, appeal, expiry sweep), branch action, 30-day removal, transparency, drill.
- `cargo test -p sr-api --test route_matrix --test openapi_names` (ok, rows reviewed, no `?`).
- `cargo test -p sr-server --test authz_matrix graduated_moderation_holds_over_http -- --include-ignored` (ok): the gate proof over HTTP on the real wiring (4-generation tree, serious ban via the staff route, only the unactivated Gen-4 node frozen, Gen 1-2 `POST /sr/invites` 201, frozen node refused 900124, banner counts only, sterilization refused for the proposer and without the approver's step-up, ok with both, +12 months, branch-action refused for every roster actor incl. a staff member with every bit but Owner, ok for the Operator with the audit entry). Whole `every_route_enforces_its_authz_row` ok (new routes seeded; no ceiling changed). `immutable_rules` 25 ok. sr-db `secret_columns`, `migration_versions`, `invite_controls`, `member_invites` ok.
- clippy `--all-targets -D warnings` on sr-admin, sr-moderation, sr-api, sr-gateway, sr-server, sr-cli, sr-db clean; `cargo fmt --all` applied. OpenAPI, api-client and MSW handlers regenerated (`mocks:check` ok).
- Web: `vitest src/admin/__tests__/p44modTools.test.tsx` (8), plus `src/i18n`, moderation, suspicion and
  `src/settings/user` (307 ok); `tsc --noEmit` clean; eslint and prettier clean on touched files.

## Not done / open

- Not run: the full web vitest suite, `build-web` and `check-bundle-budget.mjs` (only the touched areas ran: see above), `sr-gateway`/`sr-api` lib suites, Playwright.
- Operator's own 2FA in the CLI: `sr-cli` runs as the Operator on the host (like every sr-cli command) and does not ask
  for a TOTP code; the second approver's step-up is on the web. Owner question below.
- The notice is written at case open and at branch action; it is not edited as staff act on nodes later.
- `StrikeWeight` is only read for warn/timeout weights and `automated_timeout_minutes`; no automated timeout exists yet
  to scale (44-B / later waves call `automated_timeout_minutes`).
- No Playwright (not needed for the gate).

## Owner questions

1. Should `sr-cli moderation branch-action` demand the Operator's TOTP code (needs the AuthService wiring in the CLI)?
2. D-421: accounts younger than 7 days are never auto-frozen (listed only). Right default?
3. Is "serious" for the new ban route (spam, fraud, ban evasion, child safety, illegal content, platform abuse) what you meant?

## Resume here

Run the full web gates (vitest `--maxWorkers=2`, `build-web.mjs`, `check-bundle-budget.mjs`, `check-css-tokens.mjs`) and the `sr-gateway` and `sr-api` lib suites on the merged tree; wire 44-B's `StrikeWeight` with `Moderation::with_strike_weight`; then tick MOD-GRD-001 in `docs/inventory/21-feature-phase.md`.
