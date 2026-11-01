# Handoff: feat/p44-heavy-crown (wave 44-B, MOD-VCH-001 "Heavy Crown" profile vouches)

Branch `feat/p44-heavy-crown`, base `integration/phase43`. Migration 2130 only. Threat-model row T44, decisions D-425 to D-429.

## Seam with 44-A (read this first)

44-A's seam commit `41a78d8c` (`crates/sr-admin/src/serious_ban.rs`) is cherry-picked unchanged into this branch so the
real implementation compiles here (it is the first commit of `feat/p44-mod-tools`; git sees the identical change on
merge). Everything below is wired already on this branch; the integrator checks the three lines in `app.rs` survive the
merge with 44-A's own lines next to them.

| Seam (44-A) | 44-B implementation | Where it is wired |
|---|---|---|
| `sr_admin::StrikeWeight::strike_weight_multiplier(account) -> f32` (default 1.0) | `crates/sr-server/src/vouch_wiring.rs::VouchStrikeWeight` over `sr_vouch::Vouches::strike_weight_multiplier` (1.0 / 1.5 / 3.0 from the live vouch count, 1.0 on a database fault) | `crates/sr-server/src/app.rs`: `moderation.with_strike_weight(...)` |
| `sr_admin::SeriousBanObserver::serious_ban(&SeriousBan)` | `crates/sr-server/src/vouch_wiring.rs::VouchFallout`: for every account of the banned Person it calls `Vouches::on_serious_ban` (remove the vouches for the account, send each voucher a private system DM with the category only, no penalty) | `crates/sr-server/src/app.rs`: `admin_service.subscribe_serious_ban(...)` |
| AutoMod timeouts (not a 44-A seam) | `crates/sr-api/src/routes/vouch.rs::WeightedMemberTimeouter` wraps `AdminMemberTimeouter` | `crates/sr-server/src/app.rs`: `.with_timeouter(...)` |
| Sterilized members cannot vouch (44-A's `can_invite = false`) | `sr_vouch::SterilizationCheck` trait, **not wired**: 44-A's column does not exist on this base. The integrator implements `is_sterilized(account)` over 44-A's sterilization state and calls `Vouches::with_sterilization_check`. Until then nobody counts as sterilized | `crates/sr-server/src/app.rs` where `vouches` is built |

Holds: this base has no automated hold. `sr_vouch::tiers::scale_hold(base, multiplier)` (and 44-A's `scale_up`) are ready; 44-A's engine applies them when it reads `strike_weight_multiplier`.

## Done

- Migration `2130_profile_vouches.sql`: one row per (voucher, vouchee), `CHECK (voucher <> vouchee)`, a table of its own (never `arcade_build_vouches`).
- New crate `crates/sr-vouch` (no new dependency): `tiers.rs` (pure: Citizen 0-49, Pillar 50-499, Elder 500+, multiplier 1.0/1.5/3.0, upward-only scaling, 28 day timeout clamp), `service.rs` (`Vouches`: vouch, revoke, summary, count, multiplier, `on_serious_ban`, `erase`).
- Vouch rules: no self-vouch (400); the voucher must be in good standing (active, non-bot, 3 days old; 403) and not sterilized; the target must be an active member and neither side may have blocked the other (404, the same answer as a missing member); vouching twice changes nothing; at most 20 vouches less than 24 h old (409) and 500 outstanding (409); only vouchers in good standing count toward the tier.
- Routes (`crates/sr-api/src/routes/vouch.rs`): `GET /users/{id}/vouches`, `PUT /users/{id}/vouch`, `DELETE /users/{id}/vouch` (204). Reads and vouches reuse the profile privacy rules (`standing_of` + `privacy_row`), so a hidden page or a block is a 404. Bots and bearer tokens refused (`auth/policy.rs`).
- Multiplier applied to AutoMod timeouts (`WeightedMemberTimeouter`) and to 44-A's strike weight through the seam. It never lowers anything and never touches a staff decision.
- Serious ban: vouches for the banned account removed, vouchers told privately (category only, no handle), no penalty.
- GDPR: erasure (`crates/sr-db/src/account_deletion.rs`) deletes the vouches given and received in the same transaction; the data export lists them (`data_export.rs`).
- Web: `apps/web/src/profilepage/vouch/` (`VouchPanel`: count, tier badge, tooltip documenting tiers and the multiplier, Vouch / Withdraw, errors), shown on `/u/handle` and `/~handle`; i18n fragment `p44-heavy-crown-1.ts`; logical CSS only.
- Docs: T44 (and a T43 placeholder so the numbering test stays green; the integrator takes 44-A's real T43 row over it), D-425 to D-429, inventory section in `docs/inventory/21-feature-phase.md`.

## Tests run

- Rust (Postgres, scratch DB, one process, `--test-threads=1`): `sr-vouch` unit 6 + `tests/vouches.rs` 8 (pair rules, blocks, standing, sterilization stub, tiers at 49/50/499/500, multiplier, serious ban notice + removal + no penalty, erasure through `delete_account`, rate limits); `sr-server` lib `vouch_pg_tests` 2 + `vouch_wiring` 1 (the real `SeriousBanSlot` event reaches the subscriber, `StrikeWeight` seam answers 1.0/1.5/3.0); `sr-api` lib `vouch` 2 (decorator scales the AutoMod timeout); `sr-api` `route_matrix` (6) and `openapi_names` (2); `sr-server` `authz_matrix` `every_route_enforces_its_authz_row` (the three routes against every actor); `sr-server` `immutable_rules` ignored run (24); `sr-db` `secret_columns` (2), `account_deletion` + `data_export` (8).
- `cargo clippy -D warnings` on sr-vouch, sr-api, sr-server, sr-db and `cargo fmt --check`: clean.
- Web: vitest `src/profilepage` (43, incl. 8 new in `vouch.test.tsx` with an axe check, and the `/~handle` page test showing the panel), `src/i18n` (48); `tsc --noEmit`, eslint on the touched dirs, prettier, `scripts/check-css-tokens.mjs` (logical CSS) clean; `node --test scripts/threat-model.test.mjs` (5).
- Not run: `storage_scopes_hold_over_http` (untouched), the full web build and `scripts/check-bundle-budget.mjs` (needs the wasm build; the panel is in the lazy profile-page chunk), Playwright (not allowed in this wave).


## Still open (with reasons)

- Sterilization check not wired (needs 44-A's state; see the table).
- Automated holds: none exist on this base to scale.
- Signed-out `/~handle` (the public surface, `apps/web/src/public/profile`) shows no vouch panel: that bundle is isolated by D-385 and the routes need a session.
- The serious-ban notice is a best-effort system DM; there is no durable inbox row if the DM fails (logged).

## Owner questions

1. Should a member be able to refuse or remove a vouch given to them? Default (D-426): no, only the voucher can withdraw. Without it, many aged accounts can pile vouches on a member to raise their automated penalties up to 3x (T44 residual).
2. Are 20 new vouches a day, 500 outstanding and the 3-day account age right (D-425)?
3. The notice names the category and never the banned member's handle (D-428). Should it name the member?

## Resume here

Nothing half-done. If the serious-ban wiring needs changing after 44-A merges, the only code to touch is `crates/sr-server/src/vouch_wiring.rs` and three lines in `app.rs`.
