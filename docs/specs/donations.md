# Donations, perks and watch-only Bitcoin payments (as built)

Spec of record for DON-PAY, DON-ENT, DON-LB, DON-ADM and DON-UI. Product rules (prices, perks, cost model) stay in
[doc 08](../08-donations-perks-and-cost-model.md); the payment rail is [ADR-0006](../adr/0006-bitcoin-watch-only-bdk.md).
This file describes what the code does.

> **Safety statement.** The server holds no spending key and no seed. The only wallet material is an operator-supplied
> **watch-only** output descriptor (`wpkh(xpub.../0/*)`, mainnet `xpub` or test-network `tpub`). Descriptors containing a
> private key (`xprv`, `tprv`, WIF, hex secret) are rejected at parse time. No code in the repository signs or broadcasts a
> transaction. Tests use the public BIP-84 vectors and keys generated inside the test.

## 1. Architecture

| Piece | Where |
|---|---|
| Descriptor parsing, BIP-32 public derivation, bech32 addresses, BIP-21 | `crates/sr-payments/src/descriptor.rs` |
| `ChainSource` (esplora-like), in-memory `FakeChain`, `EsploraChain` over an `HttpGet` | `sr-payments/src/chain.rs` |
| `RateSource`, `RateService` (median, cache, stale-rate safety), `HttpJsonRate` | `sr-payments/src/rates.rs` |
| `DonationConfig` (prices, thresholds, perks, depths), `Limits`, `effective_limits` | `sr-payments/src/config.rs`, `limits.rs` |
| Invoice states and the pure evaluator | `sr-payments/src/invoice.rs` |
| Patron stacking, server pool and level, `Entitlements` trait and cache | `sr-payments/src/entitlements.rs` |
| `PaymentService` (create, poll, credit, resolve, grants), `PaymentRepo` port, memory repo | `sr-payments/src/service.rs`, `repo.rs`, `memory.rs` |
| Postgres repository | `sr-db/src/payments_repo.rs`, migrations `0540`..`0543` |
| HTTP | `sr-api/src/routes/donations/` (member) and `.../donations/admin.rs` |
| Watcher job, wiring, Download perk cutover | `sr-server/src/payments_jobs.rs`, `app.rs` |

The real Esplora client and the HTTP rate sources are wired from configuration; no test touches a network.

### Why no `bdk_wallet` / `bdk_esplora`
ADR-0006 names BDK. `cargo tree -i openssl` is empty for `bdk_wallet 2.4` + `bdk_esplora 0.22` (with
`async-https-rustls`), but `cargo deny check licenses` fails: every Rust Bitcoin stack (`bitcoin 0.32`, so `bdk_chain`,
`bdk_wallet`, `miniscript`) depends on `hex_lit 0.1.1`, licensed `MITNFA`, which `deny.toml` does not allow, and the stack
adds roughly 270 crates for what this feature needs: one derivation function. `sr-payments` therefore implements the
narrow part directly on `secp256k1` (CC0), `bitcoin_hashes` (CC0) and `bech32` (MIT): xpub parsing (base58check),
BIP-32 public child derivation (non-hardened), hash160 and bech32 P2WPKH addresses, BIP-380 descriptor checksums. Verified
against the BIP-84 vectors. The ADR's intent (self-custody, watch-only, esplora) is unchanged. Owner decision: accept
`MITNFA` for `hex_lit` and move to `bdk_wallet` later, or keep this implementation (see the handoff).

## 2. Data

Migrations `0540`..`0542` (times in the payment tables are Unix milliseconds):

* `payment_wallets` (network, watch-only descriptor, fingerprint, `next_index`, `gap_limit`, `active`), one active row.
* `payment_invoices` (user, purpose, guild, months, credits, `usd_cents`, `sats`, rate snapshot, address and derivation
  index, state, confirmation counters, expiry, late/excess/shortfall, `next_check_ms`, resolution), unique address and
  unique `(wallet_id, derivation_index)`.
* `payment_txs` (invoice, txid, amount, block height, first/last seen, `dropped_ms`), unique `(invoice_id, txid)`: this is
  the idempotent tx matching.
* `payment_credits` (one row per invoice, `active`, `credited_ms`, `reverted_ms`, the invite credits to reverse): the idempotency key of crediting.
* `entitlement_grants` (source invoice/admin/migrated, kind `patron` / `server_pool`, user or guild, `days`, a fixed
  `ends_ms` or `permanent`, or `usd_cents`, `starts_ms`, `revoked_ms`, reason).
* `guild_support_levels` (level, `below_since_ms`, owner and staff hiding), `user_patron_notices`, `donor_preferences`
  (hide name/amount/badge, staff hiding), `payment_config` (one JSON row), `payment_watcher` (health of the job),
  `payment_audit` (every state change, append-only).

## 3. Invoice lifecycle

States: `open`, `seen` (in mempool), `confirming` (1..depth-1 confirmations), `confirmed`, `overpaid`, `late_paid`,
`underpaid`, `expired`, `cancelled`, `resolved`.

* The pure evaluator sums the transactions to the invoice address that the chain source currently shows (sum over txids,
  so a top-up of an underpaid invoice just works). A transaction counts as *credited* once it has at least
  `confirmations` (default 1). The tolerance is 1% (`tolerance_bps` 100) both ways.
* `confirmed`: credited sum within tolerance. `overpaid`: credited sum above `sats * 1.01` (entitlement applied for the
  invoiced amount, excess recorded in `excess_sats` for staff). `late_paid`: the paying transaction was first seen after
  `expires_at` (credited and flagged). `underpaid`: something was received but not enough (staff-visible, the user can
  top up the same address). `expired`: nothing received by `expires_at` (default 60 minutes); the address keeps being
  watched for `late_watch_days` (30).
* **Reorg**: credited invoices stay watched until `finality_confirmations` (default 6). If the credited sum drops below
  the threshold (reorg, replaced tx) the credit is reverted: `payment_credits.reverted_at` is set, the grants of the invoice are
  revoked, invite-credit purchases are reversed with a negative ledger row, and `SR_ENTITLEMENT_UPDATE` follows. When the
  payment confirms again the same credit row is re-activated (still one credit per invoice).
* **Idempotent crediting**: `payment_credits.invoice_id` is the primary key; polling twice, restarting the watcher or
  racing two workers credits once. Grants carry the invoice id.
* **No automatic refunds.** Underpaid, expired-with-payment and overpaid cases are listed for staff, who resolve them by
  hand (`mark_paid`, `refund_noted`, `dismiss`) with a note. The server cannot spend; a refund is made by the operator in
  their own wallet and then recorded.
* **Gap limit**: an invoice address is index `next_index`; allocation is refused (503) when `gap_limit` (default 20)
  consecutive addresses before it have never received anything, so a restore from the xpub always finds every paid
  address. Staff raise the limit in Admin.
* **Rate**: USD cents per BTC from the operator-configured sources (median of the answers, at most 10% spread), cached
  for `rate_cache_secs`, locked into the invoice. When all sources fail the last good rate is used for at most
  `rate_max_age_secs` (900) and the invoice is flagged `rate.stale`; beyond that invoice creation fails with 503.

## 4. Entitlements and limits

* **Patron**: `months * 30` days per credited invoice, stacked after the current end; a 3-day grace after the end (shown
  as `in_grace`); up to 12 months per invoice. Staff can grant with or without an end date.
* **Server pool**: sum of credited `server_level` contributions (USD cents) with `starts_at > now - 30d` plus staff
  grants. Level = highest threshold met ($10/$25/$50). Dropping below keeps the level for `grace_days`, then the job
  downgrades (`next_level`).
* **Invite credits**: a credited `invite_credit` invoice appends `+credits` to `invite_credits_ledger` (`purchase`).
* `effective_limits(user, guild?)` = max of free defaults, the guild's level perks, and Patron perks. `Entitlements`
  (`sr_payments::Entitlements`) is the cached read trait other crates use: `user(id)`, `guild(id)`, `limits(user, guild)`.
  Cache TTL 60 s, dropped on every `SR_ENTITLEMENT_UPDATE` this process emits.
* Download perk: `sr_db::media_repo::EntitlementDownloads` implements `sr_media::downloads::DownloadEntitlements` on top
  of `Entitlements` (Patron, including grace). The legacy `cinema_download_grants` table still works as a fallback only when
  `SR_DOWNLOADS_LEGACY_FALLBACK=1` (off by default since Phase 12); migration `0542` copied the unexpired legacy grants into `entitlement_grants`
  (source `migrated`). Cutover plan in the handoff.

## 5. Gateway

* `SR_PAYMENT_UPDATE` (user-scoped, to the payer): `{type:"invoice", invoice_id, state, confirmations, received_sats,
  confirmed_sats, credited, late}` on every change. Hint only; `GET /donations/invoices/{id}` is the truth.
* `SR_ENTITLEMENT_UPDATE`: to the user (`{type:"user", user_id, patron:{active,in_grace,until_ms}, limits}`) and, for a
  server, to its members (`{type:"guild", guild_id, level, pool_cents}`; domain event `GuildEntitlementUpdated`).

## 6. HTTP API as built

All paths are relative to `/api/v1`. Ids are strings. Money is integer USD cents and satoshis. Member routes need a session
(or user bearer); admin routes need a staff session with `MANAGE_DONATIONS` (errors: 403 `SR_STAFF_ONLY` / 50013). When the
installation has no wallet or `enabled` is false: 403 `SR_FEATURE_DISABLED` on creating invoices (reads keep working).

### Member
| Route | Body / query | Result |
|---|---|---|
| `GET /donations/config` | | `{enabled, network, confirmations, invoice_expiry_minutes, patron:{price_cents, max_months, days_per_month, grace_days, perks}, levels:[{level, threshold_cents, perks}], free_perks, invite_credit_price_cents, min_donation_cents, rate:{cents_per_btc, source, age_secs, stale}\|null, hosting:{cost_cents, donated_this_month_cents}}` |
| `POST /donations/invoices` | `{purpose:"donate"\|"patron"\|"server_level"\|"invite_credit", usd_cents?, months?, guild_id?, credits?, anonymous?}` | 201 `Invoice`. `patron` needs `months` (1..12), `server_level` needs `guild_id` (member of it) and `usd_cents >= 100`, `donate` needs `usd_cents >= min`, `invite_credit` needs `credits` (1..10). 503 on gap limit or no fresh rate. 429 over 10 open invoices |
| `GET /donations/invoices` | `before?`, `limit?` (<= 50) | `{items: Invoice[], next_before}` own invoices, newest first |
| `GET /donations/invoices/{id}` | | `Invoice` (404 for other people's) |
| `POST /donations/invoices/{id}/check` | | the "I've paid" button: polls this invoice now (at most once per 10 s per invoice), returns `Invoice` |
| `POST /donations/invoices/{id}/cancel` | | 200 `Invoice`; only `open` with nothing received (409 otherwise) |
| `GET /donations/me` | | `{patron:{active,in_grace,permanent,until_ms,started_ms}, limits, preferences}` |
| `PATCH /donations/me/preferences` | `{hide_name?, hide_amount?, hide_badge?, hide_contributions?}` | preferences (`hide_contributions` keeps the account off the contributor boards) |
| `GET /guilds/{id}/support` | | `{level, pool_cents, next_threshold_cents, grace_until_ms, perks, top_contributors:[{user_id?, name?, cents?}], hidden_from_leaderboard}` (members only) |
| `PATCH /guilds/{id}/support` | `{hidden_from_leaderboard}` | guild owner only (403 for other members, 404 for non-members) |
| `GET /donations/leaderboards/contributors/{kind}` | kind `movies` \| `episodes` (Cinema) \| `chapters` (Library); `limit` (<= 100) | `{kind, items:[{rank, user_id, name, avatar?, count}], me:{rank, count}\|null, generated_at_ms}` over `contribution_credits` (credits that were not revoked; a Cinema credit counts as a movie when its item is a title and as an episode when it is an episode). Accounts that opted out or were hidden by staff are left out; cached 1 h, dropped at once when someone opts out |
| `GET /donations/leaderboards/{board}` | board `donors` \| `servers`; `window` `30d`\|`lifetime`; `limit` (<= 100) | `{board, window, items:[{rank, user_id?, guild_id?, name, avatar?, cents?, anonymous}], me:{rank, cents?}\|null, generated_at_ms}` cached 1 h. Hidden names read "Anonymous patron", hidden amounts are omitted |

`Invoice`: `{id, purpose, state, usd_cents, sats, btc, address, bip21, network, guild_id, months, credits, anonymous,
rate:{cents_per_btc, source, stale}, confirmations_required, confirmations, received_sats, confirmed_sats,
shortfall_sats, excess_sats, late, created_at_ms, expires_at_ms, quote_expires_at_ms, credited_at_ms, explorer_url,
txs:[{txid, amount_sats, confirmations, dropped}], resolution:null|{kind}}`.

### Admin (`MANAGE_DONATIONS`)
| Route | Notes |
|---|---|
| `GET /admin/donations/status` | health: wallet configured, network, fingerprint (first 8 hex of SHA-256 of the descriptor), next index, gap used/limit, last watcher run and error, chain tip, rate sources status, counts by state, unresolved count. Never returns the descriptor |
| `PUT /admin/donations/wallet` | `{descriptor, network, gap_limit?, reason}`: validates watch-only, derives and returns the first 3 addresses, activates it (older wallets keep being watched). Access log `donations.wallet.set` |
| `POST /admin/donations/wallet/test` | `{descriptor, network}` returns the first 5 receive addresses without saving |
| `GET /admin/donations/config`, `PATCH /admin/donations/config` | the `DonationConfig` (prices, thresholds, perks, depths, expiry). Patch needs `reason`; validated; access log `donations.config.set` |
| `GET /admin/donations/invoices` | `state?`, `purpose?`, `user_id?`, `q?` (id, address or txid prefix), `before?`, `limit?` |
| `GET /admin/donations/invoices/{id}` | detail with txs and the `payment_audit` trail; logs a `read` access entry |
| `POST /admin/donations/invoices/{id}/resolve` | `{resolution:"mark_paid"\|"refund_noted"\|"dismiss", note, reason}`; `mark_paid` credits the purpose once (still idempotent) |
| `GET /admin/donations/entitlements` | `kind?`, `user_id?`, `guild_id?`, `active?`, paged |
| `POST /admin/donations/entitlements` | `{kind:"patron"\|"server_pool", user_id? \| guild_id?, days? \| usd_cents?, permanent?, reason}` |
| `DELETE /admin/donations/entitlements/{id}` | reason in the body (`{reason}`) or `X-Audit-Log-Reason`; revokes |
| `GET /admin/donations/rates` | per-source status (last value, age, error) and the current median |
| `GET /admin/donations/report` | `from_ms?`, `to_ms?`: totals by purpose and by day, counts by state |
| `GET /admin/donations/leaderboard/{board}` | unfiltered ranking with real names (staff view; access-logged) |
| `POST /admin/donations/leaderboard/hide` | `{user_id? \| guild_id?, hidden, reason}` moderation |

Every admin write appends a row to the staff access log (`donations.*`) with the reason, and every state change of an
invoice appends to `payment_audit`.

## 7. Runtime and configuration

All optional; without an Esplora endpoint donations stay off (`GET /donations/config` says `enabled:false`, invoice
creation answers 403 `SR_FEATURE_DISABLED`). Misconfiguration is logged, never fatal.

| Environment variable | Meaning |
|---|---|
| `SR_PAYMENTS_ESPLORA_URL` | Esplora REST base (`https://mempool.space/api` or a self-hosted electrs/mempool). Read-only: `GET /blocks/tip/height` and `GET /address/{a}/txs` for addresses this server derived |
| `SR_PAYMENTS_RATE_SOURCES` | JSON array `[{"name","url","pointer"}]` of price sources; `pointer` is an RFC 6901 pointer to the USD price (`/data/amount`). At most 5. The median is used; sources that disagree by more than 10% are refused |
| `SR_PAYMENTS_DESCRIPTOR`, `SR_PAYMENTS_NETWORK` | A watch-only descriptor to register at startup when no wallet exists (`mainnet` default). Admin > Donations > Wallet does the same with a reason and an access-log row |
| `SR_DOWNLOADS_LEGACY_FALLBACK` | `0` (default since Phase 12): only entitlements grant the download perk; `1`: `cinema_download_grants` still does |

Jobs (`sr-server/src/payments_jobs.rs`): `payments.watch` every 15 s (polls invoices whose `next_check_ms` passed: 30 s
for open and confirming invoices, 5 min for freshly expired ones, hourly for old ones; stops at `finality_confirmations`
or after `late_watch_days`) and `payments.housekeeping` every 5 min (server level refresh with grace, Patron
reminder/grace/lapse events). Everything else in the table above that an operator tunes (prices, thresholds, perks,
depths, expiry) is `DonationConfig`, edited in Admin > Donations > Pricing and perks without a restart.

## 8. Web

* Settings > **Support / Patron** (`apps/web/src/support/`): hosting bar, Patron months, donations, invite credits,
  supporting a server, privacy switches, history and receipts. The **invoice dialog** shows sats, BTC and USD, the
  address with a copy button, a BIP-21 QR drawn by `@sideroom/ui`'s `QRCode`, a countdown, progress steps and an
  explorer link. It follows `SR_PAYMENT_UPDATE` and polls every 15 s as the fallback; "I've paid" checks now.
* Server Settings > **Server Support** (owners and `MANAGE_GUILD`): level, pool and progress, perks, top contributors,
  contribute flow, owner-only leaderboard switch.
* `/support/leaderboards`: donors and servers, 30 days or lifetime.
* Admin > **Donations**, **Invoices**, **Entitlements**, **Pricing and perks** (group MONEY, `MANAGE_DONATIONS`).

## 9. Cutover of the Cinema download perk

1. Migration `0542` copies unexpired `cinema_download_grants` rows into `entitlement_grants` (source `migrated`;
   permanent when the legacy row never expired). The legacy `per_day` is replaced by the Patron perk
   `downloads_per_day` (20).
2. `EntitlementDownloads` (sr-db) implements `sr_media::downloads::DownloadEntitlements` over `Entitlements`; with
   `SR_DOWNLOADS_LEGACY_FALLBACK=1` it still honours the old table for anyone who is not a Patron yet.
3. Since Phase 12 the fallback defaults to `0`. Staff check Admin > Entitlements before upgrading; `1` re-enables it
   temporarily. A later phase removes the flag and drops `cinema_download_grants`.

## 10. What no test or code here touched

No real chain, wallet, key or payment: tests use the public BIP-84 vector, an in-memory `FakeChain` and `FakeRate`.
The Esplora client and the HTTP rate sources are exercised only against canned responses.
