# ADR-0006: Self-custodial BTC payments with BDK watch-only wallets

- **Status:** accepted
- **Date:** 2026-10-11

## Context
Donations are crypto-only (D-071). The operator must not trust a third-party custodian, and the server should never hold spending keys.

## Decision
`sr-payments` uses **`bdk_wallet`** with an operator-supplied watch-only output descriptor (xpub) to derive one address per invoice. It watches through **`bdk_esplora`** (self-hosted electrs/mempool recommended, mempool.space as default). Confirmations drive entitlements.

## Alternatives considered
- **BTCPay Server (C#/.NET):** feature-rich but a large non-Rust dependency (4 GB RAM, 50+ GB disk pruned). Operators may still use it later via an adapter implementing the `PaymentProvider` trait.

## Consequences
We implement gap-limit handling, reorg safety, and idempotent invoice matching ourselves. Lightning (LDK) and Monero come later through the same `PaymentProvider` trait.
