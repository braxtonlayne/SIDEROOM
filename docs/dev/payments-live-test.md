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

The endpoint must be an Esplora-compatible signet API (`/blocks/tip/height`, `/address/{a}/txs`, `/tx/{id}/status`). The request goes out through the SSRF-safe client, so a private-network node needs a build that opts in. Only public chain data is read; no key or secret is involved.
