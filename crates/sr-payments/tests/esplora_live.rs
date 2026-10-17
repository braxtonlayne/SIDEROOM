//! Live Esplora checks for `EsploraChain` against a public signet node (D-257, K-09).
//! Ignored by default and skipped unless `SR_TEST_ESPLORA_URL` is set; see
//! docs/dev/payments-live-test.md.
//!
//! ```text
//! SR_TEST_ESPLORA_URL=https://mempool.space/signet/api \
//!   cargo test -p sr-payments --test esplora_live -- --ignored --test-threads=1
//! ```

use std::sync::Arc;

use sr_payments::chain::{ChainSource, EsploraChain, HttpGet, ReqwestGet};

/// Signet block 1 pays its coinbase here (a long-lived, funded watch-only address).
const FUNDED: &str = "tb1qjfplwf7a2dpjj04cx96rysqeastvycc0j50cch";
/// Block 1's coinbase transaction (height 1, 50 BTC to [`FUNDED`]).
const FIRST_TXID: &str = "8b67e4798a87162ed5f2b67a43842e4670c14409f3c1afc47d4e983532c875f5";
/// A valid signet address nobody holds the key for: a P2WPKH over 20 ASCII bytes.
fn unused_address() -> String {
    let hrp = bech32::Hrp::parse("tb").expect("hrp");
    bech32::segwit::encode_v0(hrp, b"sideroom-unused-test").expect("encode")
}

macro_rules! require_esplora {
    () => {
        match std::env::var("SR_TEST_ESPLORA_URL") {
            Ok(u) => u,
            Err(_) => {
                eprintln!(
                    "skipping: SR_TEST_ESPLORA_URL is not set (see docs/dev/payments-live-test.md)"
                );
                return;
            }
        }
    };
}

fn chain(url: &str) -> EsploraChain {
    EsploraChain::new(url, Arc::new(ReqwestGet::new(false)))
}

#[tokio::test]
#[ignore = "needs SR_TEST_ESPLORA_URL"]
async fn tip_height_is_a_number() {
    let url = require_esplora!();
    let tip = chain(&url).tip_height().await.expect("tip");
    assert!(tip > 100_000, "signet tip {tip} looks wrong");
}

#[tokio::test]
#[ignore = "needs SR_TEST_ESPLORA_URL"]
async fn funded_address_lists_confirmed_transactions() {
    let url = require_esplora!();
    let c = chain(&url);
    let tip = c.tip_height().await.expect("tip");
    let txs = c.address_txs(FUNDED).await.expect("address txs");
    assert!(!txs.is_empty(), "the funded address lists nothing");
    for t in &txs {
        assert!(t.amount_sats > 0);
        if let Some(h) = t.block_height {
            // Confirmations are the tip minus the block height plus one.
            assert_eq!(t.confirmations(tip), tip - h + 1, "{}", t.txid);
        }
    }
    // The first transaction is anchored by id and height through the raw endpoint.
    let status = ReqwestGet::new(false)
        .get(&format!(
            "{}/tx/{FIRST_TXID}/status",
            url.trim_end_matches('/')
        ))
        .await
        .expect("status");
    assert!(status.contains("\"block_height\":1"), "{status}");
}

#[tokio::test]
#[ignore = "needs SR_TEST_ESPLORA_URL"]
async fn unknown_address_lists_nothing() {
    let url = require_esplora!();
    let txs = chain(&url)
        .address_txs(&unused_address())
        .await
        .expect("address txs");
    assert!(txs.is_empty());
}
