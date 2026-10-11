# 11 — Donations & Perks (`DON`)

Phase 3. Spec: doc 08, ADR-0006.

## DON-PAY — Payments engine
- [ ] **DON-PAY-001** [DB] wallets, invoices, entitlements, server_donation_pool, donor_preferences
- [ ] **DON-PAY-002** [BE] `PaymentProvider` trait (create invoice, poll/subscribe status) for BTC now, Lightning/Monero later
- [ ] **DON-PAY-003** [BE] Descriptor/xpub validation + watch-only BDK wallet (persisted derivation index, gap limit handling)
- [ ] **DON-PAY-004** [BE] Address-per-invoice derivation, BIP21 URI builder
- [ ] **DON-PAY-005** [BE] Price oracle (two configurable sources, median, 30-min rate lock, failure fallback)
- [ ] **DON-PAY-006** [BE] Esplora watcher job (mempool seen, confirmations, reorg handling, idempotent tx matching)
- [ ] **DON-PAY-007** [BE] Invoice state machine (open/seen/confirmed/expired/underpaid/overpaid) + late-payment crediting
- [ ] **DON-PAY-008** [GW] `SR_INVOICE_UPDATE` dispatch
- [ ] **DON-PAY-009** [TEST] Signet/regtest integration tests (regtest bitcoind + electrs in CI)

## DON-ENT — Entitlements
- [ ] **DON-ENT-001** [BE] Patron entitlement (30 days per month purchased, stacking, 3-day grace)
- [ ] **DON-ENT-002** [BE] Server pool (rolling 30-day sum), level computation, grace & downgrade job
- [ ] **DON-ENT-003** [BE] Invite credit purchase → ledger +1
- [ ] **DON-ENT-004** [BE] `effective_limits(user, context)` single source of truth + moka cache + invalidation
- [ ] **DON-ENT-005** [GW] `SR_ENTITLEMENT_UPDATE`
- [ ] **DON-ENT-006** [BE] Wire limits into: upload slots, voice sender/viewer caps, stream quality caps, audio bitrate, watch party counts, download endpoints
- [ ] **DON-ENT-007** [JOB] Expiry/grace job + reminder notifications ("Your Patron perks end in 3 days")

## DON-UI — Interfaces
- [ ] **DON-UI-001** [FE] Support page `/support` (Side Room style: why donate, hosting cost vs this month's donations bar, Patron card, Server Levels explainer, invite credits, leaderboards link)
- [ ] **DON-UI-002** [FE] Invoice modal (amount USD + sats/BTC, address with copy, QR, countdown, live status steps, explorer link, "I've paid" polling)
- [ ] **DON-UI-003** [FE] Patron purchase flow (1/3/6/12 months)
- [ ] **DON-UI-004** [FE] Server contribution flow (amount presets + custom, attribution: show name / anonymous)
- [ ] **DON-UI-005** [FE] Patron badge + donor ring on avatar (toggle)
- [ ] **DON-UI-006** [FE] Upgrade hints in limit-hit toasts (upload, camera, stream quality, downloads)

## DON-LB — Leaderboards
- [ ] **DON-LB-001** [BE] Leaderboard queries (donors lifetime/30d, servers 30d, contributors movies/episodes/chapters) with privacy flags, cached hourly
- [ ] **DON-LB-002** [FE] `/support/leaderboards` page (tabs, rank, avatar/name or "Anonymous patron", amount or hidden, your rank)
- [ ] **DON-LB-003** [FE] Contributor leaderboard links from Cinema & Library footers

## DON-ADM — Admin
- [ ] **DON-ADM-001** [FE] Wallet config (descriptor, network, Esplora URL, confirmations, test derivation)
- [ ] **DON-ADM-002** [FE] Price config (Patron, L1–L3 thresholds, perks numbers per level, invite credit price, grace days)
- [ ] **DON-ADM-003** [FE] Invoices list + detail + manual resolve (mark paid / refund note)
- [ ] **DON-ADM-004** [FE] Entitlements list + manual grant/revoke
- [ ] **DON-ADM-005** [FE] Cost calculator (inputs: server cost, uplink, utilisation; outputs: suggested prices vs configured, break-even table)
