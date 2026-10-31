//! The browser side of sr-e2ee (VOX-E2E-001/002/006/007): the MLS call client (main thread),
//! the frame transform and key ring (media worker) and the privacy codes, through `@sideroom/wasm`
//! (`loadE2ee`). Same code as the native crate, checked against the shared test vectors in
//! packages/wasm/test/e2ee.test.ts (VOX-E2E-008).
//!
//! Secrets cross the boundary as byte arrays and stay inside WASM memory afterwards; nothing
//! here logs or returns key material. Randomness (the encryptor instance id) comes from the
//! caller's `crypto.getRandomValues`.

use sr_e2ee::frame::InstanceId;
use sr_e2ee::{Codec, FrameEncryptor, FrameError, KeyRing, SenderSecret};
use wasm_bindgen::prelude::*;

fn err(e: FrameError) -> JsError {
    JsError::new(&e.to_string())
}

fn codec(mime: &str) -> Result<Codec, JsError> {
    Codec::from_mime(mime).ok_or_else(|| err(FrameError::Unsupported))
}

fn secret(b: &[u8]) -> Result<SenderSecret, JsError> {
    SenderSecret::from_slice(b).ok_or_else(|| JsError::new("a sender secret is 16 bytes"))
}

/// A snowflake id (decimal string) as u64.
fn user_id(s: &str) -> Result<u64, JsError> {
    s.parse().map_err(|_| JsError::new("bad user id"))
}

/// Milliseconds or an epoch number from JS (a non-negative integer below 2^53).
fn whole(x: f64) -> Result<u64, JsError> {
    if x.is_finite() && x >= 0.0 && x.fract() == 0.0 && x <= 9_007_199_254_740_991.0 {
        Ok(x as u64)
    } else {
        Err(JsError::new("expected a non-negative integer"))
    }
}

/// Is this MIME type one the transform handles (`audio/opus`, `video/VP8`, `video/H264`)?
#[wasm_bindgen(js_name = isSupportedCodec)]
pub fn is_supported_codec(mime: &str) -> bool {
    Codec::from_mime(mime).is_some()
}

/// The local sender's encryptor for one epoch.
#[wasm_bindgen]
pub struct Encryptor(FrameEncryptor);

#[wasm_bindgen]
impl Encryptor {
    /// `secret`: this sender's 16-byte base secret for the epoch. `random`: 4 random bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(secret_bytes: &[u8], random: &[u8]) -> Result<Encryptor, JsError> {
        let r: [u8; 4] = random
            .try_into()
            .map_err(|_| JsError::new("instance randomness is 4 bytes"))?;
        FrameEncryptor::new(&secret(secret_bytes)?, InstanceId::from_random(r))
            .map(Encryptor)
            .map_err(err)
    }

    /// Encrypt one encoded frame of `mime` (an error means: drop the frame).
    pub fn encrypt(&mut self, mime: &str, frame: &[u8]) -> Result<Vec<u8>, JsError> {
        self.0.encrypt(codec(mime)?, frame).map_err(err)
    }

    /// Frames encrypted so far.
    #[wasm_bindgen(getter)]
    pub fn counter(&self) -> u32 {
        self.0.counter()
    }
}

/// The receiver's keys: the current epoch plus earlier ones for a grace window.
#[wasm_bindgen(js_name = KeyRing)]
pub struct JsKeyRing(KeyRing);

#[wasm_bindgen(js_class = KeyRing)]
impl JsKeyRing {
    #[wasm_bindgen(constructor)]
    pub fn new(grace_ms: f64) -> Result<JsKeyRing, JsError> {
        Ok(JsKeyRing(KeyRing::new(whole(grace_ms)?)))
    }

    /// Install `epoch`: `user_ids[i]` sends with `secrets[16 * i .. 16 * i + 16]`.
    #[wasm_bindgen(js_name = setEpoch)]
    pub fn set_epoch(
        &mut self,
        epoch: f64,
        user_ids: Vec<String>,
        secrets: &[u8],
        now_ms: f64,
    ) -> Result<(), JsError> {
        if secrets.len() != user_ids.len() * 16 {
            return Err(JsError::new("one 16-byte secret per user id"));
        }
        let mut senders = Vec::with_capacity(user_ids.len());
        for (i, id) in user_ids.iter().enumerate() {
            senders.push((user_id(id)?, secret(&secrets[16 * i..16 * i + 16])?));
        }
        self.0
            .set_epoch(whole(epoch)?, senders, whole(now_ms)?)
            .map_err(err)
    }

    /// Decrypt a frame from `sender` (an error means: drop the frame).
    pub fn decrypt(
        &mut self,
        sender: &str,
        mime: &str,
        frame: &[u8],
        now_ms: f64,
    ) -> Result<Vec<u8>, JsError> {
        self.0
            .decrypt(user_id(sender)?, codec(mime)?, frame, whole(now_ms)?)
            .map(|(_, plain)| plain)
            .map_err(err)
    }

    /// The current epoch, or -1 before the first.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> f64 {
        self.0.epoch().map_or(-1.0, |e| e as f64)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// The pairwise verification code between two users (D-250), the same on both sides.
#[wasm_bindgen(js_name = pairwiseCode)]
pub fn pairwise_code(
    user_a: &str,
    key_a: &[u8],
    user_b: &str,
    key_b: &[u8],
) -> Result<String, JsError> {
    Ok(sr_e2ee::privacy::pairwise_code(
        user_id(user_a)?,
        key_a,
        user_id(user_b)?,
        key_b,
    ))
}

/// The call code of the current epoch (D-250), from the MLS epoch authenticator.
#[wasm_bindgen(js_name = callCode)]
pub fn call_code(epoch_authenticator: &[u8]) -> String {
    sr_e2ee::privacy::call_code(epoch_authenticator)
}

mod mls;
pub use mls::*;

/// The local participant's encryptor for activity data messages (ACT-CH-001), one per epoch.
#[wasm_bindgen(js_name = DataSender)]
pub struct JsDataSender(sr_e2ee::data::DataSender);

#[wasm_bindgen(js_class = DataSender)]
impl JsDataSender {
    /// `secret`: this sender's 16-byte base secret for the epoch. `random`: 4 random bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(secret_bytes: &[u8], random: &[u8]) -> Result<JsDataSender, JsError> {
        let r: [u8; 4] = random
            .try_into()
            .map_err(|_| JsError::new("instance randomness is 4 bytes"))?;
        sr_e2ee::data::DataSender::new(&secret(secret_bytes)?, InstanceId::from_random(r))
            .map(JsDataSender)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Encrypt one message (an error means: do not send it).
    pub fn seal(&mut self, message: &[u8]) -> Result<Vec<u8>, JsError> {
        self.0
            .seal(message)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Messages sealed so far.
    #[wasm_bindgen(getter)]
    pub fn counter(&self) -> u32 {
        self.0.counter()
    }
}

/// Opens the other participants' activity data messages; drops replays.
#[wasm_bindgen(js_name = DataReceiver)]
pub struct JsDataReceiver(sr_e2ee::data::DataReceiver);

#[wasm_bindgen(js_class = DataReceiver)]
impl JsDataReceiver {
    #[wasm_bindgen(constructor)]
    pub fn new(grace_ms: f64) -> Result<JsDataReceiver, JsError> {
        Ok(JsDataReceiver(sr_e2ee::data::DataReceiver::new(whole(
            grace_ms,
        )?)))
    }

    /// Install `epoch`: `user_ids[i]` sends with `secrets[16 * i .. 16 * i + 16]`.
    #[wasm_bindgen(js_name = setEpoch)]
    pub fn set_epoch(
        &mut self,
        epoch: f64,
        user_ids: Vec<String>,
        secrets: &[u8],
        now_ms: f64,
    ) -> Result<(), JsError> {
        if secrets.len() != user_ids.len() * 16 {
            return Err(JsError::new("one 16-byte secret per user id"));
        }
        let mut senders = Vec::with_capacity(user_ids.len());
        for (i, id) in user_ids.iter().enumerate() {
            senders.push((user_id(id)?, secret(&secrets[16 * i..16 * i + 16])?));
        }
        self.0
            .set_epoch(whole(epoch)?, senders, whole(now_ms)?)
            .map_err(err)
    }

    /// Open one message from `sender` (an error means: drop it).
    pub fn open(&mut self, sender: &str, frame: &[u8], now_ms: f64) -> Result<Vec<u8>, JsError> {
        self.0
            .open(user_id(sender)?, frame, whole(now_ms)?)
            .map_err(|e| JsError::new(&e.to_string()))
    }
}
