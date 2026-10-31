//! Bots in voice, the end-to-end encryption half (DEV-BOT-007, D-249). Behind the `voice-e2ee` feature.
//!
//! Voice and calls on SIDE ROOM are end-to-end encrypted with MLS (docs/specs/lounge-voice-video.md section 5):
//! one MLS group per call, a per-epoch secret per sender, and a per-frame AES-GCM transform the SFU never
//! sees through. A bot that wants to hear or speak must therefore be an MLS group member. This module is that
//! member, built on the same `sr-e2ee` crate as the browser client (the Rust twin of `apps/web/src/voice/e2ee`):
//!
//! * [`Identity`]: the bot's persistent ECDSA P-256 identity key, which certifies each session's MLS key;
//! * [`E2eeCall`]: the op 101 state machine of one call. Feed it the `E2EE_MLS` dispatches and the voice-channel
//!   roster; it answers with [`Action`]s: payloads to send as gateway op 101, the sender keys of every epoch,
//!   and the `execute` that says to switch encryptors;
//! * [`E2eeMedia`]: turns those keys into frame encryption (your own frames) and decryption (everyone else's).
//!
//! The media transport (joining with gateway op 4, the `VOICE_SERVER_UPDATE` handshake with the SFU and Opus over
//! WebRTC) is `crate::voice_transport` behind the `voice-transport` feature; [`E2eeMedia`] is a
//! `voice_transport::FrameTransform` there. A bot that brings its own WebRTC stack can hand frames to
//! [`E2eeMedia`] directly.
//!
//! The module is sans-IO: no sockets, no clock of its own (callers pass `Instant`s and millisecond times) and
//! no randomness beyond what `openmls` draws.

use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use serde_json::{Value, json};
use sr_e2ee::frame::InstanceId;
use sr_e2ee::mls::{CallClient, Handled, MlsError, PendingSession};
use sr_e2ee::{Codec, FrameEncryptor, FrameError, KeyRing, SenderSecret};

/// A member other than the call's leader waits this long for a commit that nobody made before it makes one.
pub const FALLBACK: Duration = Duration::from_secs(3);

/// What went wrong in a call's key agreement.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VoiceError {
    #[error("identity key: {0}")]
    Identity(String),
    #[error("mls: {0}")]
    Mls(#[from] MlsError),
    #[error("frame: {0}")]
    Frame(FrameError),
    #[error("malformed E2EE_MLS message")]
    Malformed,
}

impl From<FrameError> for VoiceError {
    fn from(e: FrameError) -> Self {
        Self::Frame(e)
    }
}

/// The bot's persistent identity key (ECDSA P-256). Keep its secret between runs: the other members see the
/// key in the privacy code of the call, and a bot whose key changes looks like a different bot.
pub struct Identity {
    key: SigningKey,
}

impl Identity {
    /// A key from 32 secret bytes (a stored secret, or fresh randomness from the caller).
    pub fn from_secret(secret: &[u8]) -> Result<Self, VoiceError> {
        SigningKey::from_slice(secret)
            .map(|key| Self { key })
            .map_err(|e| VoiceError::Identity(e.to_string()))
    }

    /// The secret to store.
    pub fn secret(&self) -> Vec<u8> {
        self.key.to_bytes().to_vec()
    }

    /// The 65-byte uncompressed SEC1 public key.
    pub fn public_key(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec()
    }

    /// ECDSA P-256 / SHA-256, 64 bytes `r || s`.
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        let sig: Signature = self.key.sign(message);
        sig.to_bytes().to_vec()
    }
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Identity(..)")
    }
}

/// The keys of one MLS epoch, ready for [`E2eeMedia::prepare`].
pub struct EpochKeys {
    pub epoch: u64,
    /// This member's leaf: what op 101 `ready` reports.
    pub own_leaf: u32,
    /// Every member's base secret for the epoch, by user id.
    pub senders: Vec<(u64, SenderSecret)>,
}

impl std::fmt::Debug for EpochKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpochKeys")
            .field("epoch", &self.epoch)
            .field("own_leaf", &self.own_leaf)
            .field("senders", &self.senders.len())
            .finish()
    }
}

/// What the caller must do after feeding an [`E2eeCall`].
#[derive(Debug)]
pub enum Action {
    /// Send this as the `d` of gateway op 101 (`channel_id` is already in it).
    Send(Value),
    /// The group is at a new epoch: give the keys to [`E2eeMedia::prepare`]. [`E2eeCall`] has already queued the
    /// matching `ready` report.
    Keys(EpochKeys),
    /// The server says everyone is ready (or timed out): switch to the keys of `epoch` with
    /// [`E2eeMedia::execute`].
    Execute(u64),
    /// MLS refused something. The call asked to be added again with a fresh key package; do not send audio
    /// until `Keys` arrives, and never fall back to plaintext.
    Failed(String),
}

fn field<'a>(d: &'a Value, k: &str) -> Option<&'a str> {
    d.get(k).and_then(Value::as_str)
}

fn decode(d: &Value, k: &str) -> Result<Vec<u8>, VoiceError> {
    B64.decode(field(d, k).ok_or(VoiceError::Malformed)?)
        .map_err(|_| VoiceError::Malformed)
}

/// The user id of a gateway `from` field (a snowflake string).
fn user(d: &Value, k: &str) -> Option<u64> {
    field(d, k).and_then(|s| s.parse().ok())
}

/// One call's MLS session, driven by op 101 (docs: `crates/sr-gateway/src/voice_e2ee.rs`).
pub struct E2eeCall {
    me: u64,
    channel_id: String,
    client: CallClient,
    key_packages: BTreeMap<u64, Vec<u8>>,
    pending_commit: bool,
    proposals: bool,
    /// When a non-leader commits anyway.
    due: Option<Instant>,
}

impl std::fmt::Debug for E2eeCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("E2eeCall")
            .field("me", &self.me)
            .field("channel_id", &self.channel_id)
            .finish_non_exhaustive()
    }
}

impl E2eeCall {
    /// A session for the bot (user id `me`) in voice channel `channel_id`. Certifies a fresh MLS key with the
    /// identity key; send [`Self::hello`] next.
    pub fn new(me: u64, channel_id: &str, identity: &Identity) -> Result<Self, VoiceError> {
        let pending = PendingSession::generate(me, &identity.public_key())?;
        let sig = identity.sign(&pending.cert_message());
        Ok(Self {
            me,
            channel_id: channel_id.to_owned(),
            client: pending.certify(&sig)?,
            key_packages: BTreeMap::new(),
            pending_commit: false,
            proposals: false,
            due: None,
        })
    }

    fn msg(&self, d: Value) -> Action {
        let mut d = d;
        d["channel_id"] = Value::String(self.channel_id.clone());
        Action::Send(d)
    }

    /// The `d` of the gateway op 4 that joins this call's channel (D-263). It declares `sr_e2ee: true`: with
    /// `voice.e2ee` on the server refuses a join without it (`SR_VOICE_ERROR` `e2ee_required`, no token), and
    /// removes a member who does not send [`Self::hello`] within 10 seconds of `VOICE_SERVER_UPDATE`.
    /// `guild_id` is `None` for a DM call.
    pub fn join_state(&self, guild_id: Option<&str>, self_mute: bool, self_deaf: bool) -> Value {
        json!({
            "guild_id": guild_id,
            "channel_id": self.channel_id,
            "self_mute": self_mute,
            "self_deaf": self_deaf,
            "sr_e2ee": true,
        })
    }

    /// `{"type": "hello"}`: send on joining, and again on a new gateway session.
    pub fn hello(&self) -> Action {
        self.msg(json!({"type": "hello"}))
    }

    /// The epoch the group is at, once it has one.
    pub fn epoch(&self) -> Option<u64> {
        self.client.epoch().ok()
    }

    /// The call's privacy-code input: the epoch authenticator (see `sr_e2ee::privacy`).
    pub fn epoch_authenticator(&self) -> Result<Vec<u8>, VoiceError> {
        Ok(self.client.epoch_authenticator()?)
    }

    /// The members of the group with their verified identity keys (hex), by user id.
    pub fn members(&self) -> Result<Vec<(u64, String)>, VoiceError> {
        Ok(self
            .client
            .members()?
            .into_iter()
            .map(|(_, id)| (id.user_id, hex(&id.identity_key)))
            .collect())
    }

    /// The instant [`Self::poll`] wants to be called at, if a commit is due.
    pub fn deadline(&self) -> Option<Instant> {
        self.due
    }

    /// An `E2EE_MLS` dispatch for this channel (a dispatch for another channel is ignored). `roster` is the
    /// user ids in the voice channel now, the bot included.
    pub fn handle(&mut self, d: &Value, roster: &HashSet<u64>, now: Instant) -> Vec<Action> {
        if field(d, "channel_id") != Some(self.channel_id.as_str()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Err(e) = self.dispatch(d, roster, now, &mut out) {
            self.recover(&e, &mut out);
        }
        out
    }

    fn dispatch(
        &mut self,
        d: &Value,
        roster: &HashSet<u64>,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), VoiceError> {
        let from = user(d, "from");
        match field(d, "type").unwrap_or_default() {
            "call" => self.on_call(d, roster, now, out)?,
            "key_package" => {
                if let Some(u) = from.filter(|u| *u != self.me) {
                    self.key_packages.insert(u, decode(d, "data")?);
                    self.schedule(roster, now, out)?;
                }
            }
            "proposal" => {
                if self.client.has_group()
                    && self.client.handle(&decode(d, "data")?, roster)? == Handled::Proposal
                {
                    self.proposals = true;
                    self.schedule(roster, now, out)?;
                }
            }
            "commit" => self.on_commit(from, d, roster, now, out)?,
            "commit_rejected" => {
                if self.pending_commit {
                    self.client.commit_rejected()?;
                    self.pending_commit = false;
                }
                self.schedule(roster, now, out)?;
            }
            "welcome" => {
                self.client.join(&decode(d, "data")?, roster)?;
                self.pending_commit = false;
                self.on_epoch(roster, now, out)?;
            }
            "execute" => {
                if let Some(e) = d.get("epoch").and_then(Value::as_u64) {
                    out.push(Action::Execute(e));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_call(
        &mut self,
        d: &Value,
        roster: &HashSet<u64>,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), VoiceError> {
        let group_id = decode(d, "group_id")?;
        self.client.leave();
        self.pending_commit = false;
        if d.get("create").and_then(Value::as_bool) == Some(true) {
            let ds = d.get("external_sender");
            let key = ds
                .and_then(|v| field(v, "key"))
                .and_then(|s| B64.decode(s).ok());
            let id = ds
                .and_then(|v| field(v, "identity"))
                .and_then(|s| B64.decode(s).ok());
            let external = key.as_deref().zip(id.as_deref());
            self.client.create_group(&group_id, external)?;
            self.on_epoch(roster, now, out)?;
        } else {
            out.push(self.msg(
                json!({"type": "key_package", "data": B64.encode(self.client.key_package()?)}),
            ));
        }
        Ok(())
    }

    fn on_commit(
        &mut self,
        from: Option<u64>,
        d: &Value,
        roster: &HashSet<u64>,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), VoiceError> {
        if !self.client.has_group() {
            // Not in the group yet: our welcome follows.
            return Ok(());
        }
        if from == Some(self.me) && self.pending_commit {
            self.client.commit_accepted()?;
            self.pending_commit = false;
            return self.on_epoch(roster, now, out);
        }
        if let Handled::Commit { .. } = self.client.handle(&decode(d, "data")?, roster)? {
            self.pending_commit = false;
            self.on_epoch(roster, now, out)?;
        }
        Ok(())
    }

    /// A new epoch: report the keys and say `ready`.
    fn on_epoch(
        &mut self,
        roster: &HashSet<u64>,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), VoiceError> {
        let epoch = self.client.epoch()?;
        let own_leaf = self.client.own_leaf()?;
        let senders = self.client.sender_secrets()?;
        let alone = senders.len() == 1;
        out.push(Action::Keys(EpochKeys {
            epoch,
            own_leaf,
            senders,
        }));
        if alone {
            // Nobody else needs the key first.
            out.push(Action::Execute(epoch));
        }
        out.push(self.msg(json!({"type": "ready", "epoch": epoch, "leaf": own_leaf})));
        for (u, _) in self.members()? {
            self.key_packages.remove(&u);
        }
        self.proposals = false;
        self.schedule(roster, now, out)
    }

    /// The voice channel's occupants changed.
    pub fn roster_changed(&mut self, roster: &HashSet<u64>, now: Instant) -> Vec<Action> {
        self.key_packages.retain(|u, _| roster.contains(u));
        let mut out = Vec::new();
        if let Err(e) = self.schedule(roster, now, &mut out) {
            self.recover(&e, &mut out);
        }
        out
    }

    /// Call when [`Self::deadline`] has passed: a commit nobody else made is made now.
    pub fn poll(&mut self, roster: &HashSet<u64>, now: Instant) -> Vec<Action> {
        let mut out = Vec::new();
        if self.due.is_some_and(|t| now >= t) {
            self.due = None;
            if let Err(e) = self.commit(roster, &mut out) {
                self.recover(&e, &mut out);
            }
        }
        out
    }

    fn needs_commit(&self, roster: &HashSet<u64>) -> Result<bool, VoiceError> {
        if !self.client.has_group() || self.pending_commit {
            return Ok(false);
        }
        let members: Vec<u64> = self.members()?.into_iter().map(|(u, _)| u).collect();
        Ok(self.proposals
            || self
                .key_packages
                .keys()
                .any(|u| roster.contains(u) && !members.contains(u))
            || members.iter().any(|u| !roster.contains(u)))
    }

    /// The member with the lowest leaf among those still in the channel commits at once.
    fn is_leader(&self, roster: &HashSet<u64>) -> Result<bool, VoiceError> {
        let mut here: Vec<(u32, u64)> = self
            .client
            .members()?
            .into_iter()
            .filter(|(_, id)| roster.contains(&id.user_id))
            .map(|(leaf, id)| (leaf, id.user_id))
            .collect();
        here.sort();
        Ok(here.first().is_some_and(|(_, u)| *u == self.me))
    }

    fn schedule(
        &mut self,
        roster: &HashSet<u64>,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), VoiceError> {
        self.due = None;
        if !self.needs_commit(roster)? {
            return Ok(());
        }
        if self.is_leader(roster)? {
            self.commit(roster, out)
        } else {
            self.due = Some(now + FALLBACK);
            Ok(())
        }
    }

    fn commit(&mut self, roster: &HashSet<u64>, out: &mut Vec<Action>) -> Result<(), VoiceError> {
        if !self.needs_commit(roster)? {
            return Ok(());
        }
        let packages: Vec<Vec<u8>> = self.key_packages.values().cloned().collect();
        let Some(c) = self.client.commit(&packages, roster)? else {
            return Ok(());
        };
        self.pending_commit = true;
        let mut d = json!({"type": "commit", "epoch": c.based_on, "data": B64.encode(&c.commit)});
        if let Some(w) = &c.welcome {
            d["welcome"] = Value::String(B64.encode(w));
            d["to"] = json!(c.welcomed.iter().map(u64::to_string).collect::<Vec<_>>());
        }
        out.push(self.msg(d));
        Ok(())
    }

    /// MLS refused something: leave our copy of the group and ask to be added again.
    fn recover(&mut self, e: &VoiceError, out: &mut Vec<Action>) {
        self.client.leave();
        self.pending_commit = false;
        self.due = None;
        out.push(Action::Failed(e.to_string()));
        if let Ok(kp) = self.client.key_package() {
            out.push(self.msg(json!({"type": "key_package", "data": B64.encode(kp)})));
        }
    }

    /// Leave the group (left the channel).
    pub fn leave(&mut self) {
        self.client.leave();
        self.due = None;
        self.key_packages.clear();
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Frame encryption for the bot's own audio and decryption of everyone else's, following the epochs an
/// [`E2eeCall`] reports. The encryptor of a new epoch is prepared when its keys arrive and used from the
/// server's `execute` on; receivers accept the previous epoch for a grace window.
pub struct E2eeMedia {
    me: u64,
    instance: InstanceId,
    ring: KeyRing,
    next: Option<(u64, FrameEncryptor)>,
    active: Option<(u64, FrameEncryptor)>,
}

impl E2eeMedia {
    /// `instance` is four random bytes, new for every [`E2eeMedia`] (a zero byte becomes 1; it is mixed into the
    /// media key so two encryptors of one user never share a nonce space).
    pub fn new(me: u64, instance: [u8; 4]) -> Self {
        Self {
            me,
            instance: InstanceId::from_random(instance),
            ring: KeyRing::default(),
            next: None,
            active: None,
        }
    }

    /// Install the keys of a new epoch: receiving works at once, sending after [`Self::execute`].
    pub fn prepare(&mut self, keys: &EpochKeys, now_ms: u64) -> Result<(), VoiceError> {
        let mine = keys.senders.iter().find(|(u, _)| *u == self.me);
        if let Some((_, secret)) = mine {
            self.next = Some((keys.epoch, FrameEncryptor::new(secret, self.instance)?));
        }
        let senders = keys
            .senders
            .iter()
            .map(|(u, s)| (*u, SenderSecret::new(*s.as_bytes())));
        self.ring.set_epoch(keys.epoch, senders, now_ms)?;
        Ok(())
    }

    /// Switch the encryptor to `epoch` (the server's `execute`). Returns false when those keys were never
    /// prepared.
    pub fn execute(&mut self, epoch: u64) -> bool {
        match self.next.take() {
            Some((e, enc)) if e == epoch => {
                self.active = Some((e, enc));
                true
            }
            other => {
                self.next = other;
                false
            }
        }
    }

    /// The epoch frames are being encrypted under.
    pub fn sending_epoch(&self) -> Option<u64> {
        self.active.as_ref().map(|(e, _)| *e)
    }

    /// Encrypt one encoded frame; an error before the first `execute` (never send audio in the clear).
    pub fn encrypt(&mut self, codec: Codec, frame: &[u8]) -> Result<Vec<u8>, VoiceError> {
        let (_, enc) = self.active.as_mut().ok_or(FrameError::NoKey)?;
        Ok(enc.encrypt(codec, frame)?)
    }

    /// Decrypt a frame received from `sender`.
    pub fn decrypt(
        &mut self,
        sender: u64,
        codec: Codec,
        frame: &[u8],
        now_ms: u64,
    ) -> Result<Vec<u8>, VoiceError> {
        Ok(self.ring.decrypt(sender, codec, frame, now_ms)?.1)
    }

    /// Forget every key (left the call).
    pub fn clear(&mut self) {
        self.ring.clear();
        self.next = None;
        self.active = None;
    }
}

/// One message another member sent into an activity instance, opened (ACT-CH-001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataMessage {
    pub instance: u64,
    /// The sender's user id, stamped by the SFU and proven by the key that opened the message.
    pub from: u64,
    pub payload: Vec<u8>,
}

/// A bot's end of the activity data channel (ACT-CH-001), the same wire and the same sealing as the browser's
/// `sdk.state` / `sdk.input`: the SFU relays opaque bytes between the members that joined an activity instance
/// of the room, and every message is an `sr-e2ee` data frame under the sender's key for the call's epoch.
///
/// Sans-IO. Open the channel with `ConnectOptions { data: true, .. }`, wait for `VoiceEvent::DataOpen`, write
/// [`DataChannel::join`] with `VoiceSession::send_data`, then [`DataChannel::seal`] for each message and
/// [`DataChannel::open`] for each `VoiceEvent::Data`. Follow the call's epochs with [`DataChannel::prepare`] and
/// [`DataChannel::execute`], next to [`E2eeMedia`]. The Embedded App SDK's envelope (JSON with `state`, `input`
/// and `reply`) rides in the payload; a bot may use its own payloads between bots.
pub struct DataChannel {
    me: u64,
    instance: InstanceId,
    rx: sr_e2ee::data::DataReceiver,
    next: Option<(u64, sr_e2ee::data::DataSender)>,
    active: Option<(u64, sr_e2ee::data::DataSender)>,
}

const KIND_JOIN: u8 = 0;
const KIND_LEAVE: u8 = 1;
const KIND_DATA: u8 = 2;
const HEADER_OUT: usize = 17;

fn client_frame(kind: u8, instance: u64, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(9 + payload.len());
    v.push(kind);
    v.extend_from_slice(&instance.to_be_bytes());
    v.extend_from_slice(payload);
    v
}

impl DataChannel {
    /// `random`: four random bytes, new for every [`DataChannel`] (see [`E2eeMedia::new`]).
    pub fn new(me: u64, random: [u8; 4]) -> Self {
        Self {
            me,
            instance: InstanceId::from_random(random),
            rx: sr_e2ee::data::DataReceiver::new(10_000),
            next: None,
            active: None,
        }
    }

    /// Install the keys of a new epoch: opening works at once, sealing after [`Self::execute`].
    pub fn prepare(&mut self, keys: &EpochKeys, now_ms: u64) -> Result<(), VoiceError> {
        if let Some((_, secret)) = keys.senders.iter().find(|(u, _)| *u == self.me) {
            let tx = sr_e2ee::data::DataSender::new(secret, self.instance)
                .map_err(|_| VoiceError::from(FrameError::BadInstance))?;
            self.next = Some((keys.epoch, tx));
        }
        let senders = keys
            .senders
            .iter()
            .map(|(u, s)| (*u, SenderSecret::new(*s.as_bytes())));
        self.rx.set_epoch(keys.epoch, senders, now_ms)?;
        Ok(())
    }

    /// Switch sealing to `epoch` (the server's `execute`); false when those keys were never prepared.
    pub fn execute(&mut self, epoch: u64) -> bool {
        match self.next.take() {
            Some((e, tx)) if e == epoch => {
                self.active = Some((e, tx));
                true
            }
            other => {
                self.next = other;
                false
            }
        }
    }

    /// The frame that starts delivering `instance`'s messages to this member (and lets it send into it).
    pub fn join(&self, instance: u64) -> Vec<u8> {
        client_frame(KIND_JOIN, instance, &[])
    }

    pub fn leave(&self, instance: u64) -> Vec<u8> {
        client_frame(KIND_LEAVE, instance, &[])
    }

    /// Seal `payload` (at most 16 KiB) for the other members of `instance`; an error before the first
    /// [`Self::execute`] (nothing is ever sent unsealed).
    pub fn seal(&mut self, instance: u64, payload: &[u8]) -> Result<Vec<u8>, VoiceError> {
        let (_, tx) = self.active.as_mut().ok_or(FrameError::NoKey)?;
        let sealed = tx
            .seal(payload)
            .map_err(|_| VoiceError::from(FrameError::Malformed))?;
        Ok(client_frame(KIND_DATA, instance, &sealed))
    }

    /// Open a frame from `VoiceEvent::Data`. `None` for anything that must be dropped: a malformed frame, no key
    /// for the sender, a forged or replayed message.
    pub fn open(&mut self, frame: &[u8], now_ms: u64) -> Option<DataMessage> {
        if frame.len() <= HEADER_OUT || frame[0] != KIND_DATA {
            return None;
        }
        let instance = u64::from_be_bytes(frame[1..9].try_into().ok()?);
        let from = u64::from_be_bytes(frame[9..17].try_into().ok()?);
        let payload = self.rx.open(from, &frame[HEADER_OUT..], now_ms).ok()?;
        Some(DataMessage {
            instance,
            from,
            payload,
        })
    }

    /// Forget every key (left the call).
    pub fn clear(&mut self) {
        self.rx = sr_e2ee::data::DataReceiver::new(10_000);
        self.next = None;
        self.active = None;
    }
}
