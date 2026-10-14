//! The chain, as far as invoices care: which transactions pay an address and how deep they are.
//!
//! [`ChainSource`] is the esplora-like read interface. [`EsploraChain`] implements it over an
//! [`HttpGet`] (the real one is `reqwest` with rustls, configured by the operator; tests use
//! [`FakeChain`], and no test performs network I/O). Nothing here can spend or broadcast.

use crate::error::{PayError, PayResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

/// One transaction paying an address (outputs to the address are summed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTx {
    pub txid: String,
    /// Satoshis the transaction pays to the queried address.
    pub amount_sats: u64,
    /// Block height, `None` while in the mempool.
    pub block_height: Option<u32>,
}

impl ChainTx {
    /// Confirmations at `tip` (0 in the mempool).
    pub fn confirmations(&self, tip: u32) -> u32 {
        match self.block_height {
            Some(h) if tip >= h => tip - h + 1,
            Some(_) => 1,
            None => 0,
        }
    }
}

#[async_trait]
pub trait ChainSource: Send + Sync {
    /// Height of the best block.
    async fn tip_height(&self) -> PayResult<u32>;
    /// Transactions (mempool and confirmed) paying `address`. A transaction that was replaced
    /// or reorged out is simply absent.
    async fn address_txs(&self, address: &str) -> PayResult<Vec<ChainTx>>;
}

// ---- esplora -----------------------------------------------------------------------------------

/// Minimal HTTP GET (so the Esplora parsing is testable without a network).
#[async_trait]
pub trait HttpGet: Send + Sync {
    /// The body of a 2xx response; anything else is an `Err` with a short description.
    async fn get(&self, url: &str) -> Result<String, String>;
}

/// `reqwest` over rustls.
pub struct ReqwestGet {
    client: reqwest::Client,
}

impl ReqwestGet {
    pub fn new(timeout: std::time::Duration) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("sideroom-payments/1")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpGet for ReqwestGet {
    async fn get(&self, url: &str) -> Result<String, String> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }
        resp.text().await.map_err(|e| e.to_string())
    }
}

/// Esplora REST (`electrs`, mempool.space): `GET /blocks/tip/height`, `GET /address/{a}/txs`.
pub struct EsploraChain {
    base: String,
    http: Arc<dyn HttpGet>,
}

impl EsploraChain {
    pub fn new(base_url: &str, http: Arc<dyn HttpGet>) -> Self {
        Self {
            base: base_url.trim_end_matches('/').to_owned(),
            http,
        }
    }
}

#[derive(Deserialize)]
struct EsploraTx {
    txid: String,
    status: EsploraStatus,
    #[serde(default)]
    vout: Vec<EsploraOut>,
}

#[derive(Deserialize)]
struct EsploraStatus {
    confirmed: bool,
    block_height: Option<u32>,
}

#[derive(Deserialize)]
struct EsploraOut {
    scriptpubkey_address: Option<String>,
    value: u64,
}

/// Parses an Esplora `address/txs` response for `address`.
pub fn parse_address_txs(json: &str, address: &str) -> PayResult<Vec<ChainTx>> {
    let txs: Vec<EsploraTx> =
        serde_json::from_str(json).map_err(|e| PayError::Chain(format!("bad response: {e}")))?;
    Ok(txs
        .into_iter()
        .filter_map(|t| {
            let amount: u64 = t
                .vout
                .iter()
                .filter(|o| o.scriptpubkey_address.as_deref() == Some(address))
                .map(|o| o.value)
                .sum();
            (amount > 0).then_some(ChainTx {
                txid: t.txid,
                amount_sats: amount,
                block_height: if t.status.confirmed {
                    t.status.block_height
                } else {
                    None
                },
            })
        })
        .collect())
}

#[async_trait]
impl ChainSource for EsploraChain {
    async fn tip_height(&self) -> PayResult<u32> {
        let body = self
            .http
            .get(&format!("{}/blocks/tip/height", self.base))
            .await
            .map_err(PayError::Chain)?;
        body.trim()
            .parse()
            .map_err(|_| PayError::Chain("bad tip height".into()))
    }

    async fn address_txs(&self, address: &str) -> PayResult<Vec<ChainTx>> {
        let body = self
            .http
            .get(&format!("{}/address/{address}/txs", self.base))
            .await
            .map_err(PayError::Chain)?;
        parse_address_txs(&body, address)
    }
}

// ---- in-memory fake ----------------------------------------------------------------------------

#[derive(Default)]
struct FakeState {
    tip: u32,
    txs: HashMap<String, Vec<ChainTx>>,
    fail: bool,
}

/// An in-memory chain for tests and local development.
#[derive(Default, Clone)]
pub struct FakeChain {
    state: Arc<Mutex<FakeState>>,
}

impl FakeChain {
    pub fn new(tip: u32) -> Self {
        let c = Self::default();
        c.lock().tip = tip;
        c
    }

    fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A payment appears in the mempool.
    pub fn pay(&self, address: &str, txid: &str, sats: u64) {
        self.lock()
            .txs
            .entry(address.to_owned())
            .or_default()
            .push(ChainTx {
                txid: txid.to_owned(),
                amount_sats: sats,
                block_height: None,
            });
    }

    /// A payment appears already mined at `height`.
    pub fn pay_confirmed(&self, address: &str, txid: &str, sats: u64, height: u32) {
        self.lock()
            .txs
            .entry(address.to_owned())
            .or_default()
            .push(ChainTx {
                txid: txid.to_owned(),
                amount_sats: sats,
                block_height: Some(height),
            });
    }

    /// Mines `txid` into the block at `height` (and raises the tip if needed).
    pub fn confirm(&self, txid: &str, height: u32) {
        let mut s = self.lock();
        s.tip = s.tip.max(height);
        for tx in s.txs.values_mut().flatten().filter(|t| t.txid == txid) {
            tx.block_height = Some(height);
        }
    }

    pub fn set_tip(&self, tip: u32) {
        self.lock().tip = tip;
    }

    /// Mines `n` empty blocks.
    pub fn mine(&self, n: u32) {
        self.lock().tip += n;
    }

    /// The transaction vanishes (replaced, or its block was reorged out and it was not re-mined).
    pub fn drop_tx(&self, txid: &str) {
        for v in self.lock().txs.values_mut() {
            v.retain(|t| t.txid != txid);
        }
    }

    /// The transaction goes back to the mempool (its block was reorged out).
    pub fn unconfirm(&self, txid: &str) {
        for tx in self
            .lock()
            .txs
            .values_mut()
            .flatten()
            .filter(|t| t.txid == txid)
        {
            tx.block_height = None;
        }
    }

    /// Make every call fail (chain source outage).
    pub fn set_failing(&self, fail: bool) {
        self.lock().fail = fail;
    }
}

#[async_trait]
impl ChainSource for FakeChain {
    async fn tip_height(&self) -> PayResult<u32> {
        let s = self.lock();
        if s.fail {
            return Err(PayError::Chain("fake outage".into()));
        }
        Ok(s.tip)
    }

    async fn address_txs(&self, address: &str) -> PayResult<Vec<ChainTx>> {
        let s = self.lock();
        if s.fail {
            return Err(PayError::Chain("fake outage".into()));
        }
        Ok(s.txs.get(address).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"txid":"aa","status":{"confirmed":true,"block_height":100},
       "vout":[{"scriptpubkey_address":"bc1qme","value":1000},{"scriptpubkey_address":"bc1qother","value":5},{"scriptpubkey_address":"bc1qme","value":20}]},
      {"txid":"bb","status":{"confirmed":false},"vout":[{"scriptpubkey_address":"bc1qme","value":7}]},
      {"txid":"cc","status":{"confirmed":false},"vout":[{"value":9}]}
    ]"#;

    #[test]
    fn esplora_parsing_sums_outputs_to_the_address() {
        let txs = parse_address_txs(SAMPLE, "bc1qme").unwrap();
        assert_eq!(txs.len(), 2);
        assert_eq!(txs[0].amount_sats, 1020);
        assert_eq!(txs[0].confirmations(100), 1);
        assert_eq!(txs[0].confirmations(105), 6);
        assert_eq!(txs[1].block_height, None);
        assert_eq!(txs[1].confirmations(105), 0);
        assert!(parse_address_txs("nope", "x").is_err());
    }

    type Script = Vec<(String, Result<String, String>)>;
    struct Scripted(Mutex<Script>);

    #[async_trait]
    impl HttpGet for Scripted {
        async fn get(&self, url: &str) -> Result<String, String> {
            let mut v = self.0.lock().unwrap();
            let i = v
                .iter()
                .position(|(u, _)| url.ends_with(u.as_str()))
                .ok_or("unexpected url")?;
            v.remove(i).1
        }
    }

    #[tokio::test]
    async fn esplora_client_builds_urls_and_reports_errors() {
        let http = Arc::new(Scripted(Mutex::new(vec![
            ("/blocks/tip/height".into(), Ok("812345\n".into())),
            ("/address/bc1qme/txs".into(), Ok(SAMPLE.into())),
            ("/address/bad/txs".into(), Err("HTTP 500".into())),
        ])));
        let c = EsploraChain::new("https://esplora.example/api/", http);
        assert_eq!(c.tip_height().await.unwrap(), 812_345);
        assert_eq!(c.address_txs("bc1qme").await.unwrap().len(), 2);
        assert!(matches!(
            c.address_txs("bad").await,
            Err(PayError::Chain(_))
        ));
    }

    #[tokio::test]
    async fn fake_chain_models_mempool_confirm_reorg() {
        let c = FakeChain::new(10);
        c.pay("a", "t1", 500);
        assert_eq!(c.address_txs("a").await.unwrap()[0].confirmations(10), 0);
        c.confirm("t1", 11);
        assert_eq!(c.tip_height().await.unwrap(), 11);
        c.mine(2);
        assert_eq!(c.address_txs("a").await.unwrap()[0].confirmations(13), 3);
        c.unconfirm("t1");
        assert_eq!(c.address_txs("a").await.unwrap()[0].block_height, None);
        c.drop_tx("t1");
        assert!(c.address_txs("a").await.unwrap().is_empty());
        c.set_failing(true);
        assert!(c.tip_height().await.is_err());
    }
}
