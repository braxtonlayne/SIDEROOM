# ADR-0006: Self-custodial BTC payments with BDK watch-only wallets

- **Status:** accepted; amended 2026-10-15 (D-207, see Addendum)
- **Date:** 2026-10-11

## Context
Donations are crypto-only (D-071). The operator must not trust a third-party custodian, and the server should never hold spending keys.

## Decision
`sr-payments` uses **`bdk_wallet`** with an operator-supplied watch-only output descriptor (xpub) to derive one address per invoice. It watches through **`bdk_esplora`** (self-hosted electrs/mempool recommended, mempool.space as default). Confirmations drive entitlements.

## Alternatives considered
- **BTCPay Server (C#/.NET):** feature-rich but a large non-Rust dependency (4 GB RAM, 50+ GB disk pruned). Operators may still use it later via an adapter implementing the `PaymentProvider` trait.

## Consequences
We implement gap-limit handling, reorg safety, and idempotent invoice matching ourselves. Lightning (LDK) and Monero come later through the same `PaymentProvider` trait.

## Implementation note (Phase 8)
`cargo tree -i openssl` is empty for `bdk_wallet` + `bdk_esplora` (rustls feature), but the Rust Bitcoin stack (`bitcoin`, `bdk_chain`, `miniscript`) depends on `hex_lit`, whose `MITNFA` licence `deny.toml` does not allow, and the stack adds about 270 crates for one function (address derivation). `sr-payments` therefore derives addresses itself from `secp256k1`, `bitcoin_hashes` and `bech32` (BIP-32 public derivation, BIP-84 P2WPKH, BIP-380 checksums; verified against the BIP-84 vectors) and reads the chain through its own `ChainSource` trait with an Esplora client. The decision (self-custody, watch-only, Esplora) is unchanged; moving to BDK later means allowing `MITNFA` for `hex_lit` and swapping `descriptor.rs`. Details in `docs/specs/donations.md`.

## Addendum (2026-10-15, D-207)
Status: amended 2026-10-15.

`bdk_wallet` and `bdk_esplora` are replaced by in-house derivation. `sr-payments` derives addresses from `secp256k1`, `bitcoin_hashes` and `bech32` (BIP-32 public derivation, BIP-84 P2WPKH, BIP-380 checksums) and reads the chain through its own Esplora client, as described in the Phase 8 implementation note above. The decision itself (self-custody, watch-only, Esplora) is unchanged, and there is no `MITNFA` exception in `deny.toml`.

Revisit trigger: move to BDK when a BDK release no longer pulls in `hex_lit` (so no licence exception is needed), or when the product needs descriptors beyond `wpkh` (multisig, taproot, miniscript policies). Until then the in-house code stays; swapping means replacing `descriptor.rs` only.
