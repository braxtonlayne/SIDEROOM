# Handoff: feat/p8-donations (self-custodial Bitcoin donations, entitlements, leaderboards)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p8-donations`, branch `feat/p8-donations` (base `integration/phase7` = main).
Built with `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p4-admin-ui-target` and `CARGO_PROFILE_DEV_DEBUG=0`.
Spec of record, written with the code: `docs/specs/donations.md` (HTTP API as built, section 6).

## Read this first: what was and was not touched
**No real chain, wallet, key or payment was touched.** Every test uses the public BIP-84 test vector (re-serialised as a
`tpub` for the test networks), an in-memory `FakeChain` and `FakeRate`, or canned Esplora JSON. The real Esplora client
(`EsploraChain` over `reqwest` with rustls) and the HTTP price sources are wired from environment variables and have
never spoken to a network from this branch. The server holds no spending key or seed, there is no code that signs or
broadcasts, and a descriptor containing private key material is refused by the parser, by the API and by a CHECK
constraint on `payment_wallets`. First contact with a real Esplora needs: `GET /blocks/tip/height` and
`GET /address/{a}/txs` against a signet/testnet endpoint, with a signet wallet, before any mainnet descriptor is set.

## Done
- **`crates/sr-payments`** (pure; no database, no network):
  `descriptor` (xpub/tpub parsing, BIP-32 public derivation, P2WPKH bech32, BIP-380 checksum, BIP-21 builder; verified
  against the BIP-84 vectors and the BIP-380 checksum vector), `chain` (`ChainSource`, `EsploraChain`, `FakeChain`),
  `rates` (`RateSource`, `HttpJsonRate`, `RateService`: median, cache, spread guard, stale-rate safety),
  `config` (`DonationConfig`, `PerkSet`, `Limits`, `effective_limits`), `invoice` (states and the pure evaluator),
  `entitlements` (Patron stacking with grace, pool window, level with grace, `Entitlements` trait, moka-cached
  `EntitlementService`), `repo` (`PaymentRepo` port) with `memory` (reference implementation), `service`
  (`PaymentService`, `PaymentProvider` trait).
- **Migrations 0540..0542** and **`sr-db::payments_repo::PgPaymentRepo`**: credit and revert in one transaction under the
  invoice row lock; `entitlement_grants` as the source of truth; the legacy download table copied (cutover below).
- **Gateway**: `SR_PAYMENT_UPDATE` (payer) and `SR_ENTITLEMENT_UPDATE` (user shape, and guild shape through the new
  `DomainEvent::GuildEntitlementUpdated`), documented in doc 05; new `StaffAlertKind::PaymentAttention`.
- **API** (`sr-api/src/routes/donations/`): 11 member routes, 15 admin routes (all `MANAGE_DONATIONS`, reasons required,
  access-log rows `donations.*`, invoice detail reads logged as `read`). `sr_admin::AdminService::record_read` and
  `require_reason` were added for this.
- **Server**: `payments_jobs.rs` (watcher every 15 s, housekeeping every 5 min), environment configuration, and
  `EntitlementDownloads` replacing `PgDownloadEntitlements` as the `DownloadEntitlements` source.
- **Web**: Settings > Support / Patron, the invoice dialog (QR, copy, countdown, steps, live status, polling fallback,
  receipt), Server Settings > Server Support, `/support/leaderboards`, Admin > Donations / Invoices / Entitlements /
  Pricing and perks (with the doc 08 cost calculator and leaderboard moderation).
- openapi.json and the api-client regenerated.
- Inventory ticked: DON-PAY-001..008, DON-ENT-001..005 and 007, DON-UI-002..004, DON-ADM-001..005; notes on the partial ones.

## Cutover of the Cinema download perk
Migration 0542 copies unexpired `cinema_download_grants` into `entitlement_grants` (source `migrated`). The server uses
`EntitlementDownloads`; `SR_DOWNLOADS_LEGACY_FALLBACK=1` (default) still honours the old table for non-Patrons. After
checking Admin > Entitlements, set it to `0`; a later migration can drop the table. The legacy per-day number is replaced
by the Patron perk `downloads_per_day` (20, editable in Pricing and perks). Server levels never unlock downloads (D-075).

## Decisions (please review)
1. **No `bdk_wallet` / `bdk_esplora`.** `cargo tree -i openssl` is empty for both (rustls feature), but `cargo deny
   check licenses` fails on `hex_lit 0.1.1` (`MITNFA`), which every Rust Bitcoin stack (`bitcoin`, `bdk_chain`,
   `miniscript`) pulls in, and the stack adds about 270 crates for one function. Derivation is implemented on
   `secp256k1` (CC0), `bitcoin_hashes` (CC0) and `bech32` (MIT): no `hex_lit`, no openssl, `cargo deny check licenses
   bans sources` pass. Owner choice: accept `MITNFA` for `hex_lit` (an `exceptions` entry in `deny.toml`) and move to
   BDK later, or keep this. Note `secp256k1-sys` compiles C (like `libwebp-sys`).
2. **Only `wpkh(xpub/tpub .../<branch>/*)` descriptors** (plus an origin and `<0;1>` multipath, first branch used), no
   hardened steps after the key, no `tr`, `sh`, multisig. Enough for single-sig watch-only wallets (Sparrow, Electrum,
   Trezor/Ledger exports).
3. **Amount is locked for the whole invoice life** (60 min). The 30-minute "rate lock" is shown to the payer; a payment
   that arrives between 30 and 60 minutes is judged against the original sats. The operator carries up to 60 minutes of
   price risk. Both numbers are in `DonationConfig`.
4. **Tolerance 1% both ways**; a top-up of an underpaid invoice goes to the same address and is summed.
5. **Leaderboards**: donors can hide name and/or amount (opt-in anonymity, D-074); invite-credit purchases do not count
   as donations on the board but do count in the hosting bar; a server's contributors are listed with the contributor's
   own choice (anonymous invoice or hidden name). Boards are cached for an hour. Contributor boards (movies, episodes,
   chapters) are not built: `contribution_credits` does not exist yet (REV-CORE-006).
6. **Guild level also writes `guilds.boost_level`** (the existing column) so a Discord-shaped `premium_tier` can read it.
7. **Config lives in one JSON row** (`payment_config`) instead of the `platform_settings` catalog, because the catalog
   is integer/boolean only and gated by `MANAGE_FEATURES`; the money screens need nested perk tables and
   `MANAGE_DONATIONS`.
8. **Reorg revert** of an `invite_credit` purchase subtracts the credits with a `staff_revoke` ledger row, which can
   leave a negative balance if they were already spent. The window is the confirmation depth (1) until
   `finality_confirmations` (6); raise `confirmations` to shrink it.
9. The existing `/support` route is the entrance help page, not the donation page. The donation "Support page" is
   Settings > Support / Patron; `/support/leaderboards` is new.
10. The watch-only descriptor is stored as text in `payment_wallets` and never returned by the API (only its 8-character fingerprint); it is not a secret, but anyone with database access can list every donation address and balance.
11. Esplora serves at most 25 confirmed transactions per address page; an invoice address with more than that (it
    should not happen) would need paging.

## Not done / gaps
- DON-PAY-009 (regtest/signet integration tests in CI): needs bitcoind + electrs.
- DON-ENT-006 beyond downloads: upload slots, voice sender/viewer caps, stream quality, audio bitrate and watch-party
  counts must call `Entitlements::limits(user, guild)` in their own crates (the trait is ready; voice should also drop
  its cache on `SR_ENTITLEMENT_UPDATE`). No other crate was edited.
- DON-UI-005 (Patron badge and donor ring on avatars), DON-UI-006 (upgrade hints in limit toasts), DON-LB-003 (footer
  links), contributor boards, avatars on the leaderboard.
- Lightning and Monero (the `PaymentProvider` trait exists; only Bitcoin implements it).
- Reminder notifications are `SR_ENTITLEMENT_UPDATE` events (reason `expiring`), not notification-centre items.
- No e2e spec was written or run (ports 8080/5173 are shared).
- The rate source URLs come from the environment, not from Admin; there is no UI to edit them (Admin shows their status).

## Tests
Counts: sr-payments 56, sr-api donations 6, web 39 new (full web suite 1107 passing, workspace `cargo test` green). Postgres-backed (ignored): `cargo test -p sr-db --test payments_repo -- --ignored
--test-threads=1` (7). Unit/scenario tests: `cargo test -p sr-payments` (descriptor vectors, state machine, reorg
revert, idempotency, rate staleness, gap limit, grace, events), `cargo test -p sr-api donations` (6 endpoint tests with
the 401/403 matrices and the access-log trail). Web: `src/support/__tests__`, `src/settings/server/__tests__/support.test.tsx`,
`src/admin/__tests__/donations.test.tsx`, `src/admin/donations/calculator.test.ts` (axe on the main screens).

## Gates and dependency findings
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`: green.
  `cargo deny check`: bans, licenses and sources pass. **Advisories fail on the base branch too** (RUSTSEC-2026-0346..0349 on
  `rustls 0.23.45`, unrelated to this work): needs a `cargo update -p rustls` by the owner.
- New crates in `sr-payments`: `secp256k1 0.29` (CC0-1.0, builds C like `libwebp-sys`), `bitcoin_hashes 0.14` (CC0-1.0),
  `bech32 0.11` (MIT), plus `reqwest 0.13` (already used elsewhere, rustls), `moka`, `futures`. `cargo tree -i openssl` and
  `cargo tree -i hex_lit` are both empty. No new npm dependency (the QR code is `@sideroom/ui`'s existing `QRCode`).
- Web: typecheck, eslint, `prettier --check .`, production build and the bundle budget pass; the new screens are in the
  lazy admin and settings chunks.
- Postgres: `cargo test -p sr-db --test payments_repo --test media_repo -- --ignored --test-threads=1` (7 + 8) and
  `cargo test -p sr-server -- --ignored --test-threads=1` (7) pass on a throwaway database.

## Owner decisions needed
1. `MITNFA` for `hex_lit` (BDK later) or keep the in-house derivation. 2. Whether the 60-minute price risk (decision 3)
is acceptable or the dialog should require a refreshed quote after 30 minutes. 3. When to turn off
`SR_DOWNLOADS_LEGACY_FALLBACK` and drop `cinema_download_grants`. 4. Which Esplora endpoint and price sources the
installation uses (self-hosted electrs is recommended; mempool.space leaks the invoice addresses to a third party).
5. Whether Cinema/Library footers should link the leaderboards before contributor boards exist.
