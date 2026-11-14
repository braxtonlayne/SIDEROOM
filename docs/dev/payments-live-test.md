# Live Esplora test (sr-payments)

CI covers the Esplora client with `FakeChain`, recorded response parsing and the BIP-84 vectors. It does not run `bitcoind` or `electrs` (D-257, DON-PAY-009 "won't build in CI"). `crates/sr-payments/tests/esplora_live.rs` instead checks `EsploraChain` against a public signet Esplora. It is opt-in and run by hand before a release that touches `sr-payments`.

```sh
SR_TEST_ESPLORA_URL=https://mempool.space/signet/api \
  cargo test -j 2 -p sr-payments --test esplora_live -- --ignored --test-threads=1
```

Without `SR_TEST_ESPLORA_URL` the tests print a skip message and pass.

| Test | Proves |
|---|---|
| `tip_height_is_a_number` | `GET /blocks/tip/height` parses |
| `funded_address_lists_confirmed_transactions` | a known funded watch-only signet address (the block 1 coinbase address) lists transactions; confirmations equal tip minus block height plus one; the block 1 coinbase txid is at height 1 |
| `unknown_address_lists_nothing` | an unused address answers an empty list, not an error |

The endpoint must be an Esplora-compatible signet API (`/blocks/tip/height`, `/address/{a}/txs`, `/tx/{id}/status`). The request goes out through the SSRF-safe client with private hosts refused (`ReqwestGet::new(false)`), so the test endpoint must be publicly routable; a private-network node cannot be used by this test. The server's own Esplora client takes the private-host opt-in from the `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS` env var (`crates/sr-server/src/payments_jobs.rs`). Only public chain data is read; no key or secret is involved.

## The end-to-end walk (RDY-PAY-001, D-368)

The check above reads the chain only. This is the whole path: an invoice, a real signet payment, the watcher confirming it, the credit and the entitlement. It is run by hand, once, on the first real host (or the prod-smoke stack) before the first paid invite, by the owner. CI does not run it (D-257). What the code does at each step is in [the donations spec](../specs/donations.md) (sections 3, 4 and 6).

**Before you start**
- A signet wallet with a few thousand sats in it (a faucet pays one). A signet **watch-only** descriptor of a second wallet, for the server (`wpkh(tpub.../0/*)`, never a private key; the server rejects `tprv`, `xprv` and WIF). Use a different wallet to pay from than the one the descriptor belongs to, or the payment shows in both.
- An Esplora endpoint for signet: `SR_PAYMENTS_ESPLORA_URL=https://mempool.space/signet/api` (or your own electrs on signet). A private host needs `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1`. Rate sources are the defaults (`SR_PAYMENTS_RATE_SOURCES`); a signet coin has no price, so the quote uses the mainnet rate, which is fine for the walk.
- Restart `sr-server` after setting the environment. `GET /donations/config` (or Admin > Donations) must say `enabled: true`.

**The walk**
1. **Register the wallet.** Admin > Donations > Wallet: paste the signet descriptor, network `signet`, a reason. The page shows the first three addresses; check the first against your wallet. The health panel shows the network, the fingerprint, the next index and a chain tip that moves. Optionally set the explorer link in Admin > Donations > Pricing and perks to `https://mempool.space/signet/tx/{txid}`.
2. **Create the invoice.** Sign in as an ordinary member (not the Owner, so the entitlement shows as a member sees it). Settings > Support > Patron, 1 month, Pay. The invoice dialog shows sats, BTC and USD, the address, a QR code and a countdown (60 minutes by default). Note the invoice id and the address.
3. **Pay it.** From the other wallet, on **signet**, send the exact amount to that address (the dialog's BIP-21 link or QR does it). Write down the transaction id.
4. **Watch it confirm.** Without reloading, the dialog moves through "seen" (in the mempool), "confirming" and "confirmed" as `SR_PAYMENT_UPDATE` arrives; "I've paid" polls at once (at most every 10 seconds). With the default `confirmations = 1` it confirms at the first signet block (about 10 minutes, sometimes longer). The watcher job `payments.watch` runs every 15 seconds, so a missed push is caught within a minute.
5. **See the credit and the entitlement.** The member's Settings > Support shows Patron active with an end date 30 days out; `GET /donations/me` says `patron.active: true`. A Patron perk is visible (for example the Cinema download allowance). The invoice appears in the member's history and receipts.
6. **Check Admin.** As staff with `MANAGE_DONATIONS`: Admin > Donations > Invoices lists the invoice as `confirmed` with the transaction, its confirmations and the `payment_audit` trail (opening it writes a `read` entry in the access log); Entitlements lists the Patron grant with the invoice as its source; the health panel shows no unresolved invoice and a recent watcher run. Also check that exactly one credit exists: press "I've paid" again and reload; nothing doubles.
7. **Optional, once the walk has passed:** pay a second invoice 1% short and see it land in `underpaid`, then top it up to the same address (the sum is evaluated, so it confirms); or pay after the invoice expired to see `late_paid`. Skip these if the first walk used up your sats.

**What to record.** Date, host, network, the invoice id, the transaction id, the block height it confirmed at, and what you saw at step 5 and 6. Add a row to the table below; do not add a row for a run that did not happen.

**How to undo it.** The server cannot spend or refund: signet coins are worth nothing, so there is nothing to give back. In Admin > Donations > Entitlements revoke the Patron grant (a reason is required; it is logged), and if you want the invoice out of the unresolved list resolve it with `dismiss` and a note. If the wallet was only for the walk, replace it with the real mainnet wallet in Admin > Donations > Wallet (older wallets keep being watched; check the health panel shows the new network). Remove the signet Esplora URL from the environment and restart before the first real invite.

## Recorded runs

| Date | Host | Network | Invoice id | Transaction id | Confirmed at block | Who | Notes |
|---|---|---|---|---|---|---|---|
