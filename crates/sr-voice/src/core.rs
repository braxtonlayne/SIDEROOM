//! The sans-IO SFU core: rooms, participants, SDP negotiation and audio forwarding.
//!
//! Nothing here touches a socket, a clock or a task. The caller feeds it received
//! datagrams ([`SfuCore::handle_input`]), the time ([`SfuCore::handle_timeout`]) and
//! signalling ([`SfuCore::handle_offer`], ...), and drains [`CoreOutput`]s with
//! [`SfuCore::poll`]. That keeps the whole media path testable in memory.
//!
//! # Model
//!
//! * A **room** is a voice channel; it exists while it has participants.
//! * A **participant** owns one `str0m::Rtc` (ICE-lite, DTLS-SRTP). Its microphone
//!   arrives on one inbound audio m-line (`mic`).
//! * Every other participant in the room is a **subscriber** of that microphone through
//!   its own `sendonly` audio m-line whose msid stream id is the speaker's user id
//!   ([`OutTrack`]). Negotiating those lines is the SFU's job (server offers).
//!
//! # Later (not built)
//!
//! Video, simulcast and screen share extend `TrackSource` (today only `Mic`) and give
//! `OutTrack` a layer selector driven by the subscriber's viewport hints, its bandwidth
//! estimate and the egress governor (doc 08 section 3). The forwarding hook is
//! [`SfuCore::forward_audio`]; the future one sits beside it. End-to-end encryption needs no
//! change here: frames are forwarded opaque and only header extensions are read.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, GuildId, UserId};
use sr_voice_proto::wire::{self, Message, ParticipantInfo, Speaking, UserRef, op, speaking};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::media::{Direction, MediaData, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};
use thiserror::Error;

/// Largest SDP the core will parse (browsers send a few KiB).
pub const MAX_SDP_BYTES: usize = 64 * 1024;
/// After push-to-talk is released, audio still passes this long (late packets, reordering).
pub const PTT_GRACE: Duration = Duration::from_millis(300);
/// An ICE-disconnected participant is removed after this long without recovering
/// (an ICE restart from the client revives it before then).
pub const ICE_DISCONNECT_GRACE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParticipantId(pub u64);

/// Media-path permissions, set by the gateway (token claims, then [`VoiceCommand`](sr_voice_proto::VoiceCommand)s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// May send audio: has SPEAK and is not server muted.
    pub speak: bool,
    /// Receives nothing (server or self deafened).
    pub deaf: bool,
    /// No USE_VAD: audio passes only while the client reports push-to-talk.
    pub ptt_required: bool,
    /// PRIORITY_SPEAKER.
    pub priority: bool,
}

pub struct JoinParams {
    pub user: UserId,
    pub guild: Option<GuildId>,
    pub channel: ChannelId,
    pub flags: Flags,
    /// Local candidates to advertise in the SDP answer (ICE-lite, so these are all there is).
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Error)]
pub enum SfuError {
    #[error("unknown participant")]
    UnknownParticipant,
    #[error("the SFU is full")]
    Full,
    #[error("the room is full")]
    RoomFull,
    #[error("bad SDP: {0}")]
    Sdp(String),
    #[error("bad candidate: {0}")]
    Candidate(String),
    #[error("rtc error: {0}")]
    Rtc(String),
}

/// Why a participant left the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// [`SfuCore::leave`] with no special cause (client gone, WebSocket resume window over).
    Left,
    /// A moderator or the gateway removed them (move, kick, leave, session loss).
    Disconnected,
    /// The same user joined again.
    Replaced,
    /// ICE failed and did not recover, or the Rtc died.
    Connection,
    /// The room was closed (channel deleted) or the SFU is shutting down.
    RoomClosed,
}

/// A datagram to put on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutPacket {
    pub proto: Protocol,
    pub source: SocketAddr,
    pub destination: SocketAddr,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    /// A voice-WebSocket message for one participant.
    Signal { to: ParticipantId, msg: Message },
    /// The participant is gone from the core; close its WebSocket.
    Closed {
        pid: ParticipantId,
        user: UserId,
        channel: ChannelId,
        reason: CloseReason,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreOutput {
    Transmit(OutPacket),
    Event(CoreEvent),
}

/// Node-level numbers behind the Prometheus gauges.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Snapshot {
    pub rooms: usize,
    pub participants: usize,
    pub egress_bps: f64,
    pub ingress_bps: f64,
    /// Mean packet-loss fraction (0..1) the receivers report for what we send them.
    pub loss_ratio: f64,
    pub forwarded_packets: u64,
}

/// What a remote m-line carries. Only the microphone exists today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TrackSource {
    Mic,
    // TODO(video): Camera, Screen, ScreenAudio.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutState {
    ToOpen,
    Negotiating(Mid),
    Open(Mid),
    ToStop(Mid),
    NegotiatingStop(Mid),
}

impl OutState {
    fn mid(self) -> Option<Mid> {
        match self {
            Self::ToOpen => None,
            Self::Negotiating(m) | Self::Open(m) | Self::ToStop(m) | Self::NegotiatingStop(m) => {
                Some(m)
            }
        }
    }
}

/// One participant's subscription to another's track.
struct OutTrack {
    #[allow(dead_code)] // read by the future video selector
    source: TrackSource,
    state: OutState,
}

#[derive(Default)]
struct Rates {
    last: Option<(Instant, u64, u64)>,
    rx_bps: f64,
    tx_bps: f64,
    loss: Option<f32>,
}

struct Participant {
    user: UserId,
    guild: Option<GuildId>,
    channel: ChannelId,
    flags: Flags,
    rtc: Rtc,
    /// Inbound microphone m-line.
    mic: Option<Mid>,
    /// Subscriptions, keyed by the speaker.
    out: HashMap<ParticipantId, OutTrack>,
    /// A server offer waiting for the client's answer.
    pending: Option<SdpPendingOffer>,
    /// The client has sent at least one offer, so server offers are possible.
    negotiated: bool,
    detector: crate::speaking::SpeakingDetector,
    /// Last push-to-talk state the client reported (`speaking` bitmask) and when it last dropped to 0.
    ptt_bits: u8,
    ptt_released_at: Option<Instant>,
    next_timeout: Option<Instant>,
    ice_down_since: Option<Instant>,
    dead: Option<CloseReason>,
    rates: Rates,
}

impl Participant {
    /// Whether audio from this participant is forwarded right now.
    fn may_send(&self, now: Instant) -> bool {
        if !self.flags.speak {
            return false;
        }
        if !self.flags.ptt_required {
            return true;
        }
        self.ptt_bits != 0
            || self
                .ptt_released_at
                .is_some_and(|t| now.saturating_duration_since(t) <= PTT_GRACE)
    }

    fn speaking_bits(&self) -> u8 {
        if !self.detector.is_speaking() {
            return 0;
        }
        speaking::MICROPHONE
            | if self.flags.priority {
                speaking::PRIORITY
            } else {
                0
            }
    }
}

pub struct SfuCore {
    parts: HashMap<ParticipantId, Participant>,
    rooms: HashMap<ChannelId, Vec<ParticipantId>>,
    by_user: HashMap<UserId, ParticipantId>,
    by_addr: HashMap<SocketAddr, ParticipantId>,
    out: VecDeque<CoreOutput>,
    next_id: u64,
    max_participants: usize,
    max_room_size: usize,
    forwarded: u64,
}

impl SfuCore {
    pub fn new(max_participants: usize, max_room_size: usize) -> Self {
        // Idempotent: the first call wins process-wide.
        str0m::crypto::from_feature_flags().install_process_default();
        Self {
            parts: HashMap::new(),
            rooms: HashMap::new(),
            by_user: HashMap::new(),
            by_addr: HashMap::new(),
            out: VecDeque::new(),
            next_id: 1,
            max_participants,
            max_room_size,
            forwarded: 0,
        }
    }

    // ---- queries ----------------------------------------------------------------------

    pub fn participant_count(&self) -> usize {
        self.parts.len()
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    pub fn participant_of(&self, user: UserId) -> Option<ParticipantId> {
        self.by_user.get(&user).copied()
    }

    pub fn channel_of(&self, pid: ParticipantId) -> Option<ChannelId> {
        self.parts.get(&pid).map(|p| p.channel)
    }

    pub fn guild_of(&self, pid: ParticipantId) -> Option<GuildId> {
        self.parts.get(&pid).and_then(|p| p.guild)
    }

    pub fn user_of(&self, pid: ParticipantId) -> Option<UserId> {
        self.parts.get(&pid).map(|p| p.user)
    }

    pub fn flags_of(&self, pid: ParticipantId) -> Option<Flags> {
        self.parts.get(&pid).map(|p| p.flags)
    }

    pub fn is_connected(&self, pid: ParticipantId) -> bool {
        self.parts.get(&pid).is_some_and(|p| p.rtc.is_connected())
    }

    /// The other participants of `pid`'s room, as READY lists them.
    pub fn others(&self, pid: ParticipantId) -> Vec<ParticipantInfo> {
        let Some(me) = self.parts.get(&pid) else {
            return Vec::new();
        };
        self.rooms
            .get(&me.channel)
            .into_iter()
            .flatten()
            .filter(|id| **id != pid)
            .filter_map(|id| self.parts.get(id))
            .map(|p| ParticipantInfo {
                user_id: p.user,
                speaking: p.speaking_bits(),
                muted: !p.flags.speak,
                deafened: p.flags.deaf,
            })
            .collect()
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut s = Snapshot {
            rooms: self.rooms.len(),
            participants: self.parts.len(),
            forwarded_packets: self.forwarded,
            ..Snapshot::default()
        };
        let (mut loss, mut n) = (0.0f64, 0u32);
        for p in self.parts.values() {
            s.egress_bps += p.rates.tx_bps;
            s.ingress_bps += p.rates.rx_bps;
            if let Some(l) = p.rates.loss {
                loss += f64::from(l);
                n += 1;
            }
        }
        if n > 0 {
            s.loss_ratio = loss / f64::from(n);
        }
        s
    }

    /// Next datagram or event, if any.
    pub fn poll(&mut self) -> Option<CoreOutput> {
        self.out.pop_front()
    }

    /// When [`handle_timeout`](Self::handle_timeout) must run next.
    pub fn next_timeout(&self) -> Option<Instant> {
        self.parts
            .values()
            .flat_map(|p| {
                [
                    p.next_timeout,
                    p.detector.deadline(),
                    p.ice_down_since.map(|t| t + ICE_DISCONNECT_GRACE),
                ]
            })
            .flatten()
            .min()
    }

    // ---- joining and leaving ------------------------------------------------------------

    /// Create a participant. A second join of the same user replaces the first.
    pub fn join(&mut self, now: Instant, p: JoinParams) -> Result<ParticipantId, SfuError> {
        if let Some(old) = self.by_user.get(&p.user).copied() {
            self.remove(old, CloseReason::Replaced);
        }
        if self.parts.len() >= self.max_participants {
            return Err(SfuError::Full);
        }
        if self.rooms.get(&p.channel).map_or(0, Vec::len) >= self.max_room_size {
            return Err(SfuError::RoomFull);
        }
        let mut rtc = Rtc::builder()
            .set_ice_lite(true)
            // Audio only for now; video codecs come back with the video work.
            .clear_codecs()
            .enable_opus(true, false)
            .set_stats_interval(Some(Duration::from_secs(1)))
            .build(now);
        for c in p.candidates {
            rtc.add_local_candidate(c);
        }
        let pid = ParticipantId(self.next_id);
        self.next_id += 1;
        let room = self.rooms.entry(p.channel).or_default();
        let existing = room.clone();
        room.push(pid);
        self.by_user.insert(p.user, pid);
        self.parts.insert(
            pid,
            Participant {
                user: p.user,
                guild: p.guild,
                channel: p.channel,
                flags: p.flags,
                rtc,
                mic: None,
                out: HashMap::new(),
                pending: None,
                negotiated: false,
                detector: crate::speaking::SpeakingDetector::default(),
                ptt_bits: 0,
                ptt_released_at: None,
                next_timeout: None,
                ice_down_since: None,
                dead: None,
                rates: Rates::default(),
            },
        );
        // Existing participants whose microphone is already open become this one's subscriptions.
        for other in existing {
            let has_mic = self.parts.get(&other).is_some_and(|o| o.mic.is_some());
            if has_mic && let Some(me) = self.parts.get_mut(&pid) {
                me.out.insert(
                    other,
                    OutTrack {
                        source: TrackSource::Mic,
                        state: OutState::ToOpen,
                    },
                );
            }
            self.signal(other, op::CLIENT_CONNECT, &UserRef { user_id: p.user });
        }
        Ok(pid)
    }

    /// Remove a participant (idempotent).
    pub fn leave(&mut self, pid: ParticipantId, reason: CloseReason) {
        self.remove(pid, reason);
    }

    /// Remove the participant `user` has in `channel`, if any.
    pub fn disconnect_user(&mut self, channel: ChannelId, user: UserId, reason: CloseReason) {
        if let Some(pid) = self.by_user.get(&user).copied()
            && self.parts.get(&pid).is_some_and(|p| p.channel == channel)
        {
            self.remove(pid, reason);
        }
    }

    pub fn close_room(&mut self, channel: ChannelId) {
        for pid in self.rooms.get(&channel).cloned().unwrap_or_default() {
            self.remove(pid, CloseReason::RoomClosed);
        }
    }

    pub fn close_all(&mut self) {
        let channels: Vec<_> = self.rooms.keys().copied().collect();
        for c in channels {
            self.close_room(c);
        }
    }

    fn remove(&mut self, pid: ParticipantId, reason: CloseReason) {
        let Some(mut p) = self.parts.remove(&pid) else {
            return;
        };
        p.rtc.disconnect();
        self.by_user.remove(&p.user);
        self.by_addr.retain(|_, v| *v != pid);
        if let Some(room) = self.rooms.get_mut(&p.channel) {
            room.retain(|x| *x != pid);
            if room.is_empty() {
                self.rooms.remove(&p.channel);
            }
        }
        let members = self.rooms.get(&p.channel).cloned().unwrap_or_default();
        for m in members {
            if let Some(q) = self.parts.get_mut(&m) {
                // The speaker's line is stopped by the next negotiation (see `negotiate`).
                if matches!(
                    q.out.get(&pid).map(|t| t.state),
                    Some(OutState::ToOpen) | None
                ) {
                    q.out.remove(&pid);
                }
            }
            if p.detector.is_speaking() {
                self.signal(
                    m,
                    op::SPEAKING,
                    &Speaking {
                        user_id: Some(p.user),
                        speaking: 0,
                        audio_level: None,
                    },
                );
            }
            self.signal(m, op::CLIENT_DISCONNECT, &UserRef { user_id: p.user });
        }
        self.out.push_back(CoreOutput::Event(CoreEvent::Closed {
            pid,
            user: p.user,
            channel: p.channel,
            reason,
        }));
    }

    // ---- signalling -----------------------------------------------------------------------

    /// Apply a client offer and return the answer SDP. The caller sends it as `SDP_ANSWER`.
    pub fn handle_offer(
        &mut self,
        now: Instant,
        pid: ParticipantId,
        sdp: &str,
    ) -> Result<String, SfuError> {
        if sdp.len() > MAX_SDP_BYTES {
            return Err(SfuError::Sdp("SDP too large".into()));
        }
        let offer = SdpOffer::from_sdp_string(sdp).map_err(|e| SfuError::Sdp(e.to_string()))?;
        let p = self
            .parts
            .get_mut(&pid)
            .ok_or(SfuError::UnknownParticipant)?;
        // The client's offer wins a collision: the server offer is abandoned and re-made
        // after this exchange (the server is the polite peer, spec 4.4).
        if p.pending.take().is_some() {
            for t in p.out.values_mut() {
                t.state = match t.state {
                    OutState::Negotiating(_) => OutState::ToOpen,
                    OutState::NegotiatingStop(m) => OutState::ToStop(m),
                    s => s,
                };
            }
        }
        let answer = p
            .rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| SfuError::Rtc(e.to_string()))?;
        p.negotiated = true;
        self.pump(now);
        Ok(answer.to_sdp_string())
    }

    /// Apply the client's answer to a server offer.
    pub fn handle_answer(
        &mut self,
        now: Instant,
        pid: ParticipantId,
        sdp: &str,
    ) -> Result<(), SfuError> {
        if sdp.len() > MAX_SDP_BYTES {
            return Err(SfuError::Sdp("SDP too large".into()));
        }
        let answer = SdpAnswer::from_sdp_string(sdp).map_err(|e| SfuError::Sdp(e.to_string()))?;
        let p = self
            .parts
            .get_mut(&pid)
            .ok_or(SfuError::UnknownParticipant)?;
        // No pending offer: a stale answer to an abandoned offer. Ignore.
        let Some(pending) = p.pending.take() else {
            return Ok(());
        };
        p.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|e| SfuError::Rtc(e.to_string()))?;
        for t in p.out.values_mut() {
            if let OutState::Negotiating(m) = t.state {
                t.state = OutState::Open(m);
            }
        }
        p.out
            .retain(|_, t| !matches!(t.state, OutState::NegotiatingStop(_)));
        self.pump(now);
        Ok(())
    }

    /// A trickled remote candidate. ICE-lite learns peer-reflexive addresses on its own,
    /// so this is a courtesy; malformed lines are reported but harmless.
    pub fn add_remote_candidate(
        &mut self,
        pid: ParticipantId,
        candidate_line: &str,
    ) -> Result<(), SfuError> {
        let line = candidate_line.trim_start_matches("a=");
        let c = Candidate::from_sdp_string(line).map_err(|e| SfuError::Candidate(e.to_string()))?;
        let p = self
            .parts
            .get_mut(&pid)
            .ok_or(SfuError::UnknownParticipant)?;
        p.rtc.add_remote_candidate(c);
        Ok(())
    }

    /// Re-send the server offer the client may have missed while its WebSocket was down.
    pub fn resend_pending_offer(&mut self, pid: ParticipantId) {
        // The offer text is not kept (str0m owns the pending state), so abandon it and re-make it.
        if let Some(p) = self.parts.get_mut(&pid)
            && p.pending.take().is_some()
        {
            for t in p.out.values_mut() {
                t.state = match t.state {
                    OutState::Negotiating(_) => OutState::ToOpen,
                    OutState::NegotiatingStop(m) => OutState::ToStop(m),
                    s => s,
                };
            }
        }
    }

    /// The client's `SPEAKING` bitmask (push-to-talk state).
    pub fn set_client_speaking(&mut self, now: Instant, pid: ParticipantId, bits: u8) {
        if let Some(p) = self.parts.get_mut(&pid) {
            if bits == 0 && p.ptt_bits != 0 {
                p.ptt_released_at = Some(now);
            }
            p.ptt_bits = bits;
        }
    }

    /// Apply new media-path flags (server mute, deafen, permission change).
    /// Returns whether `user` is in `channel` (and so the flags were applied).
    pub fn set_flags(
        &mut self,
        now: Instant,
        channel: ChannelId,
        user: UserId,
        flags: Flags,
    ) -> bool {
        let Some(pid) = self.by_user.get(&user).copied() else {
            return false;
        };
        let Some(p) = self.parts.get_mut(&pid) else {
            return false;
        };
        if p.channel != channel {
            return false;
        }
        let was_speaking = p.detector.is_speaking();
        p.flags = flags;
        // A speaker who may no longer send stops speaking for everyone at once.
        if !p.may_send(now) && p.detector.reset() && was_speaking {
            self.announce_speaking(pid, false, None);
        }
        true
    }

    // ---- network -----------------------------------------------------------------------------

    /// A datagram arrived. Returns whether some participant accepted it.
    pub fn handle_input(
        &mut self,
        now: Instant,
        proto: Protocol,
        source: SocketAddr,
        destination: SocketAddr,
        data: &[u8],
    ) -> bool {
        let Ok(contents) = data.try_into() else {
            return false;
        };
        let input = Input::Receive(
            now,
            Receive {
                proto,
                source,
                destination,
                contents,
            },
        );
        let cached = self
            .by_addr
            .get(&source)
            .copied()
            .filter(|id| self.parts.get(id).is_some_and(|p| p.rtc.accepts(&input)));
        let pid = cached.or_else(|| {
            self.parts
                .iter()
                .find(|(_, p)| p.rtc.accepts(&input))
                .map(|(id, _)| *id)
        });
        let Some(pid) = pid else {
            return false;
        };
        self.by_addr.insert(source, pid);
        if let Some(p) = self.parts.get_mut(&pid)
            && p.rtc.handle_input(input).is_err()
        {
            p.dead = Some(CloseReason::Connection);
        }
        self.pump(now);
        true
    }

    /// Advance time for everyone whose timer is due.
    pub fn handle_timeout(&mut self, now: Instant) {
        let ids: Vec<_> = self.parts.keys().copied().collect();
        for id in ids {
            let mut stopped = false;
            if let Some(p) = self.parts.get_mut(&id) {
                if p.next_timeout.is_none_or(|t| t <= now)
                    && p.rtc.handle_input(Input::Timeout(now)).is_err()
                {
                    p.dead = Some(CloseReason::Connection);
                }
                if p.detector.on_tick(now) == Some(false) {
                    stopped = true;
                }
                if p.ice_down_since
                    .is_some_and(|t| now.saturating_duration_since(t) >= ICE_DISCONNECT_GRACE)
                {
                    p.dead = Some(CloseReason::Connection);
                }
            }
            if stopped {
                self.announce_speaking(id, false, None);
            }
        }
        self.pump(now);
    }

    // ---- internals ------------------------------------------------------------------------------

    fn signal(&mut self, to: ParticipantId, op: u8, d: &impl serde::Serialize) {
        self.out.push_back(CoreOutput::Event(CoreEvent::Signal {
            to,
            msg: Message::new(op, d),
        }));
    }

    fn announce_speaking(&mut self, src: ParticipantId, on: bool, level: Option<i8>) {
        let Some(p) = self.parts.get(&src) else {
            return;
        };
        let bits = if on {
            speaking::MICROPHONE
                | if p.flags.priority {
                    speaking::PRIORITY
                } else {
                    0
                }
        } else {
            0
        };
        let msg = Speaking {
            user_id: Some(p.user),
            speaking: bits,
            audio_level: if on { level } else { None },
        };
        let audience: Vec<_> = self
            .rooms
            .get(&p.channel)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| *id != src)
            .collect();
        for to in audience {
            self.signal(to, op::SPEAKING, &msg);
        }
    }

    /// Drive every Rtc until nothing more happens, forwarding as media arrives.
    fn pump(&mut self, now: Instant) {
        // Bounded: forwarding feeds other Rtcs, which can then have output of their own.
        for _ in 0..32 {
            let mut work = false;
            let ids: Vec<_> = self.parts.keys().copied().collect();
            for id in ids {
                if self.negotiate(id) {
                    work = true;
                }
                let mut events = Vec::new();
                if let Some(p) = self.parts.get_mut(&id) {
                    loop {
                        match p.rtc.poll_output() {
                            Ok(Output::Transmit(t)) => {
                                self.out.push_back(CoreOutput::Transmit(OutPacket {
                                    proto: t.proto,
                                    source: t.source,
                                    destination: t.destination,
                                    data: t.contents.to_vec(),
                                }));
                            }
                            Ok(Output::Event(e)) => events.push(e),
                            Ok(Output::Timeout(t)) => {
                                p.next_timeout = Some(t);
                                break;
                            }
                            Err(_) => {
                                p.dead = Some(CloseReason::Connection);
                                break;
                            }
                        }
                        if !p.rtc.is_alive() {
                            p.dead = Some(CloseReason::Connection);
                            break;
                        }
                    }
                }
                if !events.is_empty() {
                    work = true;
                }
                for e in events {
                    self.on_event(id, e, now);
                }
            }
            let dead: Vec<_> = self
                .parts
                .iter()
                .filter_map(|(id, p)| p.dead.map(|r| (*id, r)))
                .collect();
            for (id, reason) in dead {
                self.remove(id, reason);
                work = true;
            }
            if !work {
                break;
            }
        }
    }

    fn on_event(&mut self, id: ParticipantId, e: Event, now: Instant) {
        match e {
            Event::IceConnectionStateChange(s) => {
                if let Some(p) = self.parts.get_mut(&id) {
                    p.ice_down_since = match s {
                        IceConnectionState::Disconnected => p.ice_down_since.or(Some(now)),
                        _ => None,
                    };
                }
            }
            Event::MediaAdded(m) => {
                if m.kind != MediaKind::Audio || !m.direction.is_receiving() {
                    return;
                }
                let Some(p) = self.parts.get_mut(&id) else {
                    return;
                };
                if p.mic.is_some() {
                    return;
                }
                p.mic = Some(m.mid);
                let channel = p.channel;
                // The new microphone becomes a subscription for everyone else.
                let others: Vec<_> = self
                    .rooms
                    .get(&channel)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|o| *o != id)
                    .collect();
                for o in others {
                    if let Some(q) = self.parts.get_mut(&o) {
                        q.out.entry(id).or_insert(OutTrack {
                            source: TrackSource::Mic,
                            state: OutState::ToOpen,
                        });
                    }
                }
            }
            Event::MediaData(d) => self.forward_audio(id, d, now),
            Event::PeerStats(s) => {
                if let Some(p) = self.parts.get_mut(&id) {
                    if let Some((t0, rx0, tx0)) = p.rates.last {
                        let dt = s.timestamp.saturating_duration_since(t0).as_secs_f64();
                        if dt > 0.0 {
                            p.rates.rx_bps =
                                (s.peer_bytes_rx.saturating_sub(rx0)) as f64 * 8.0 / dt;
                            p.rates.tx_bps =
                                (s.peer_bytes_tx.saturating_sub(tx0)) as f64 * 8.0 / dt;
                        }
                    }
                    p.rates.last = Some((s.timestamp, s.peer_bytes_rx, s.peer_bytes_tx));
                    p.rates.loss = s.egress_loss_fraction;
                }
            }
            Event::Closed => {
                if let Some(p) = self.parts.get_mut(&id) {
                    p.dead = Some(CloseReason::Connection);
                }
            }
            // TODO(video): KeyframeRequest coalescing, EgressBitrateEstimate -> layer selection.
            _ => {}
        }
    }

    /// Forward one microphone frame to the room, and run speaking detection.
    fn forward_audio(&mut self, src: ParticipantId, d: MediaData, now: Instant) {
        let Some(p) = self.parts.get_mut(&src) else {
            return;
        };
        if p.mic != Some(d.mid) || !p.may_send(now) {
            return;
        }
        let channel = p.channel;
        let level = d.ext_vals.audio_level;
        let vad = d.ext_vals.voice_activity;
        let transition = p.detector.on_packet(now, level, vad);
        if let Some(on) = transition {
            self.announce_speaking(src, on, level);
        }
        let targets: Vec<_> = self
            .rooms
            .get(&channel)
            .into_iter()
            .flatten()
            .copied()
            .filter(|o| *o != src)
            .collect();
        for o in targets {
            let Some(q) = self.parts.get_mut(&o) else {
                continue;
            };
            if q.flags.deaf {
                continue;
            }
            // TODO(egress-governor): consult the room budget here before writing.
            let Some(mid) = q.out.get(&src).and_then(|t| t.state.mid()) else {
                continue;
            };
            let Some(mut w) = q.rtc.writer(mid) else {
                continue;
            };
            // Pass the level on so receiving clients can draw meters without decoding.
            if let (Some(l), v) = (level, vad) {
                w = w.audio_level(l, v.unwrap_or(true));
            }
            let Some(pt) = w.match_params(d.params) else {
                continue;
            };
            if w.write(pt, d.network_time, d.time, d.data.clone()).is_ok() {
                self.forwarded += 1;
            }
        }
    }

    /// Start a server offer for `id` if its subscriptions changed. Returns whether it did.
    fn negotiate(&mut self, id: ParticipantId) -> bool {
        let alive: HashSet<ParticipantId> = self.parts.keys().copied().collect();
        let users: HashMap<ParticipantId, UserId> =
            self.parts.iter().map(|(k, v)| (*k, v.user)).collect();
        let Some(p) = self.parts.get_mut(&id) else {
            return false;
        };
        if !p.negotiated || p.pending.is_some() {
            return false;
        }
        // Lines whose speaker left are stopped; lines not yet opened for a gone speaker are dropped.
        let gone: Vec<_> = p
            .out
            .keys()
            .copied()
            .filter(|s| !alive.contains(s))
            .collect();
        for s in gone {
            match p.out.get(&s).map(|t| t.state) {
                Some(OutState::Open(m)) => {
                    if let Some(t) = p.out.get_mut(&s) {
                        t.state = OutState::ToStop(m);
                    }
                }
                Some(OutState::ToOpen) => {
                    p.out.remove(&s);
                }
                _ => {}
            }
        }
        if !p
            .out
            .values()
            .any(|t| matches!(t.state, OutState::ToOpen | OutState::ToStop(_)))
        {
            return false;
        }
        let mut change = p.rtc.sdp_api();
        for (src, t) in p.out.iter_mut() {
            match t.state {
                OutState::ToOpen => {
                    let stream = users.get(src).map(ToString::to_string);
                    let mid =
                        change.add_media(MediaKind::Audio, Direction::SendOnly, stream, None, None);
                    t.state = OutState::Negotiating(mid);
                }
                OutState::ToStop(m) => {
                    change.stop_media(m);
                    t.state = OutState::NegotiatingStop(m);
                }
                _ => {}
            }
        }
        let Some((offer, pending)) = change.apply() else {
            return false;
        };
        p.pending = Some(pending);
        let sdp = offer.to_sdp_string();
        self.signal(id, op::SDP_OFFER, &wire::Sdp { sdp });
        true
    }
}
