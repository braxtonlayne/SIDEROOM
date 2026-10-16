//! The MLS call client for the browser (VOX-E2E-001), over `sr_e2ee::mls`. MLS messages go in
//! and out as base64 strings, the form op 101 carries them in. The per-sender secrets come out
//! as bytes only, for the media worker (which zeroes its copy).

use std::collections::HashSet;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::json;
use sr_e2ee::mls::{self, Handled, MlsError};
use wasm_bindgen::prelude::*;

fn mls_err(e: MlsError) -> JsError {
    JsError::new(&e.to_string())
}

fn unb64(s: &str) -> Result<Vec<u8>, JsError> {
    B64.decode(s).map_err(|_| JsError::new("bad base64"))
}

fn roster(ids: &[String]) -> Result<HashSet<u64>, JsError> {
    ids.iter().map(|s| crate::user_id(s)).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Step 1: a fresh session signature key, waiting for the identity key's signature.
#[wasm_bindgen]
pub struct PendingSession(Option<mls::PendingSession>);

#[wasm_bindgen]
impl PendingSession {
    /// `identity_key`: the account's P-256 identity public key, 65-byte uncompressed SEC1
    /// (WebCrypto `exportKey('raw')`).
    #[wasm_bindgen(constructor)]
    pub fn new(user: &str, identity_key: &[u8]) -> Result<PendingSession, JsError> {
        mls::PendingSession::generate(crate::user_id(user)?, identity_key)
            .map(|p| PendingSession(Some(p)))
            .map_err(mls_err)
    }

    /// What the identity key signs (ECDSA P-256 with SHA-256).
    #[wasm_bindgen(js_name = certMessage)]
    pub fn cert_message(&self) -> Result<Vec<u8>, JsError> {
        self.0
            .as_ref()
            .map(|p| p.cert_message())
            .ok_or_else(|| JsError::new("already certified"))
    }

    /// Step 2: the signature (64 bytes, r||s as WebCrypto returns it).
    pub fn certify(&mut self, signature: &[u8]) -> Result<CallClient, JsError> {
        let p = self
            .0
            .take()
            .ok_or_else(|| JsError::new("already certified"))?;
        p.certify(signature).map(CallClient).map_err(mls_err)
    }
}

/// One member's MLS state for one call.
#[wasm_bindgen]
pub struct CallClient(mls::CallClient);

#[wasm_bindgen]
impl CallClient {
    #[wasm_bindgen(js_name = keyPackage)]
    pub fn key_package(&self) -> Result<String, JsError> {
        self.0.key_package().map(|b| B64.encode(b)).map_err(mls_err)
    }

    /// Start the group alone. `external_key`/`external_identity` (base64): the delivery
    /// service's external sender, from the gateway's `call` message.
    #[wasm_bindgen(js_name = createGroup)]
    pub fn create_group(
        &mut self,
        group_id: &str,
        external_key: Option<String>,
        external_identity: Option<String>,
    ) -> Result<(), JsError> {
        let gid = unb64(group_id)?;
        let ext = match (external_key, external_identity) {
            (Some(k), Some(i)) => Some((unb64(&k)?, unb64(&i)?)),
            _ => None,
        };
        self.0
            .create_group(
                &gid,
                ext.as_ref().map(|(k, i)| (k.as_slice(), i.as_slice())),
            )
            .map_err(mls_err)
    }

    #[wasm_bindgen(getter, js_name = hasGroup)]
    pub fn has_group(&self) -> bool {
        self.0.has_group()
    }

    /// The current epoch, or -1 without a group.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> f64 {
        self.0.epoch().map_or(-1.0, |e| e as f64)
    }

    #[wasm_bindgen(js_name = ownLeaf)]
    pub fn own_leaf(&self) -> Result<u32, JsError> {
        self.0.own_leaf().map_err(mls_err)
    }

    #[wasm_bindgen(js_name = epochAuthenticator)]
    pub fn epoch_authenticator(&self) -> Result<Vec<u8>, JsError> {
        self.0.epoch_authenticator().map_err(mls_err)
    }

    /// `[{leaf, user_id, identity_key (hex)}]` as JSON.
    pub fn members(&self) -> Result<String, JsError> {
        let m = self.0.members().map_err(mls_err)?;
        let v: Vec<_> = m
            .iter()
            .map(|(leaf, id)| {
                json!({
                    "leaf": leaf,
                    "user_id": id.user_id.to_string(),
                    "identity_key": hex(&id.identity_key),
                })
            })
            .collect();
        Ok(serde_json::Value::Array(v).to_string())
    }

    /// The user ids of [`Self::sender_secrets`], in the same order.
    #[wasm_bindgen(js_name = senderUserIds)]
    pub fn sender_user_ids(&self) -> Result<Vec<String>, JsError> {
        Ok(self
            .0
            .sender_secrets()
            .map_err(mls_err)?
            .iter()
            .map(|(u, _)| u.to_string())
            .collect())
    }

    /// Every member's 16-byte base secret for this epoch, concatenated.
    #[wasm_bindgen(js_name = senderSecrets)]
    pub fn sender_secrets(&self) -> Result<Vec<u8>, JsError> {
        let s = self.0.sender_secrets().map_err(mls_err)?;
        let mut out = Vec::with_capacity(s.len() * 16);
        for (_, secret) in &s {
            out.extend_from_slice(secret.as_bytes());
        }
        Ok(out)
    }

    /// Commit adds for `key_packages` and removals of members not in `roster`. JSON
    /// `{based_on, commit, welcome, welcomed}` or `null` when there is nothing to commit.
    pub fn commit(
        &mut self,
        key_packages: Vec<String>,
        roster_ids: Vec<String>,
    ) -> Result<String, JsError> {
        let kps = key_packages
            .iter()
            .map(|k| unb64(k))
            .collect::<Result<Vec<_>, _>>()?;
        let out = self
            .0
            .commit(&kps, &roster(&roster_ids)?)
            .map_err(mls_err)?;
        Ok(match out {
            None => "null".into(),
            Some(c) => json!({
                "based_on": c.based_on,
                "commit": B64.encode(&c.commit),
                "welcome": c.welcome.map(|w| B64.encode(w)),
                "welcomed": c.welcomed.iter().map(u64::to_string).collect::<Vec<_>>(),
            })
            .to_string(),
        })
    }

    /// The server accepted our commit; returns the new epoch.
    #[wasm_bindgen(js_name = commitAccepted)]
    pub fn commit_accepted(&mut self) -> Result<f64, JsError> {
        self.0.commit_accepted().map(|e| e as f64).map_err(mls_err)
    }

    #[wasm_bindgen(js_name = commitRejected)]
    pub fn commit_rejected(&mut self) -> Result<(), JsError> {
        self.0.commit_rejected().map_err(mls_err)
    }

    /// Another member's commit or a proposal. JSON `{type: "commit", epoch}`,
    /// `{type: "proposal"}` or `{type: "ignored"}`.
    pub fn handle(&mut self, message: &str, roster_ids: Vec<String>) -> Result<String, JsError> {
        let h = self
            .0
            .handle(&unb64(message)?, &roster(&roster_ids)?)
            .map_err(mls_err)?;
        Ok(match h {
            Handled::Commit { epoch } => json!({ "type": "commit", "epoch": epoch }),
            Handled::Proposal => json!({ "type": "proposal" }),
            Handled::Ignored => json!({ "type": "ignored" }),
        }
        .to_string())
    }

    /// Join from a welcome; returns the epoch.
    pub fn join(&mut self, welcome: &str, roster_ids: Vec<String>) -> Result<f64, JsError> {
        self.0
            .join(&unb64(welcome)?, &roster(&roster_ids)?)
            .map(|e| e as f64)
            .map_err(mls_err)
    }

    pub fn leave(&mut self) {
        self.0.leave();
    }
}
