//! The sans-IO SFU core: rooms, participants, SDP negotiation, and audio and video forwarding.
//!
//! Nothing here touches a socket, a clock or a task. The caller feeds it received
//! datagrams ([`SfuCore::handle_input`]), the time ([`SfuCore::handle_timeout`]) and
//! signalling ([`SfuCore::handle_offer`], ...), and drains [`CoreOutput`]s with
//! [`SfuCore::poll`]. That keeps the whole media path testable in memory.
//!
//! # Model
//!
//! * A **room** is a voice channel; it exists while it has participants.
//! * A **participant** owns one `str0m::Rtc` (ICE-lite, DTLS-SRTP) and publishes up to four
//!   **tracks**, each on its own inbound m-line ([`TrackSource`]): the microphone, the
//!   camera (simulcast, layers `q`/`h`/`f`), the screen, the screen audio and a stereo
//!   music track (MUS-JB-001). The client
//!   names which m-line is which with a `VIDEO` publish message ([`SfuCore::handle_video`]).
//! * Every other participant in the room is a **subscriber**. It receives each source it
//!   wants through its own `sendonly` m-line ([`OutTrack`]); the SFU offers those lines
//!   ([`SfuCore::negotiate`]). Which sources a subscriber gets is decided in one place,
//!   [`SfuCore::reconcile`]:
//!   * the microphone: always;
//!   * the camera: unless the client turned it off, at the layer the client's hint, its
//!     bandwidth estimate and the egress governor allow ([`crate::layers`]);
//!   * the screen and its audio: only when the client subscribed **and** the gateway listed it
//!     as a viewer of that stream ([`SfuCore::set_stream_viewers`]), so the viewer cap cannot
//!     be bypassed from the client side. The two lines share a msid stream id
//!     (`<user>-screen`), which makes the browser play them as one synchronized stream group.
//! * The music track goes to every other member (msid stream id `<user>-music`) unless the
//!   subscriber turned it off with a hint, and only while the room allows music
//!   ([`SfuCore::set_music`]) and the publisher has SPEAK. It is forwarded opaque like all
//!   audio (D-201) and only after the publisher's E2EE handshake, the same gate as the
//!   microphone (D-249); its bytes feed the governor's egress like everything else and are
//!   also counted separately ([`Snapshot::music_egress_bps`]).
//! * A publisher may only send what the gateway allows (`Flags::camera`, `Flags::screen`,
//!   set from the sender allowance, VOX-SIG-009). Anything else it sends is dropped.
//!
//! # Video forwarding
//!
//! Frames are forwarded as str0m delivers them (depacketized), never decoded, and
//! re-packetized for each subscriber. Per subscriber and source a [`LayerSelector`] says
//! which simulcast layer flows; a layer switch waits for a keyframe of the new layer, which
//! the core asks the publisher for through a [`KeyframeCoalescer`] (so a crowd of
//! subscribers costs the publisher one keyframe per half second). PLI and FIR received from
//! subscribers go through the same coalescer. NACK and RTX are handled by str0m from its
//! send buffer. End-to-end encryption needs no change here: frames are forwarded opaque and
//! only header extensions are read (the keyframe flag comes from the codec payload
//! descriptor, which E2EE leaves in the clear).
//!
//! # Egress governor
//!
//! Once a second the core feeds the node's egress to the [`EgressGovernor`], which lowers the
//! layer cap tier by tier when the node runs over its budget. Affected subscribers get a
//! `QUALITY` message.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, GuildId, UserId};
use sr_voice_proto::sdp::{MONO_OPUS_PT, STEREO_OPUS_PT, expand_opus_lines, restrict_opus_lines};
use sr_voice_proto::video::{
    Layer, MUSIC_DEFAULT_KBPS, Source, SubscriptionHint, Video, clamp_music_kbps,
};
use sr_voice_proto::wire::{self, Message, ParticipantInfo, Speaking, UserRef, op, speaking};
use str0m::bwe::Bitrate;
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::format::{Codec, FormatParams};
use str0m::media::{Direction, Frequency, KeyframeRequestKind, MediaData, MediaKind, Mid, Pt, Rid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};
use thiserror::Error;

use crate::governor::{EgressGovernor, GovernorConfig, Tier, cap_for, notice};

/// The layer cap a member gets: the governor's for its tier, unless the channel is on full video
/// quality (J-11), where camera layers stay up to 720p whatever the room load.
fn cap_of(severity: u8, flags: &Flags) -> Layer {
    if flags.video_full {
        Layer::High
    } else {
        cap_for(severity, Tier(flags.tier))
    }
}
use crate::keyframe::{self, KeyframeCoalescer};
use crate::layers::{Available, Frame, LayerSelector, SelectorConfig, layer_for_viewport};
use crate::loss::LossBudget;

/// Largest SDP the core will parse (browsers send a few KiB).
pub const MAX_SDP_BYTES: usize = 64 * 1024;
/// After push-to-talk is released, audio still passes this long (late packets, reordering).
pub const PTT_GRACE: Duration = Duration::from_millis(300);
/// An ICE-disconnected participant is removed after this long without recovering
/// (an ICE restart from the client revives it before then).
pub const ICE_DISCONNECT_GRACE: Duration = Duration::from_secs(15);
/// How often layer selection, the keyframe timer and the governor run.
pub const MAINTAIN_EVERY: Duration = Duration::from_millis(200);
/// How long a simulcast layer counts as "being sent" after its last frame.
const LAYER_LIVE: Duration = Duration::from_secs(3);
/// Starting bandwidth estimate for a new subscriber (str0m's BWE needs one).
const INITIAL_BWE: Bitrate = Bitrate::kbps(1_000);

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
    /// May publish a camera (the sender allowance let it start one).
    pub camera: bool,
    /// May publish a screen share and its audio.
    pub screen: bool,
    /// Which allowance the participant is on, for the egress governor ([`Tier`]).
    pub tier: u8,
    /// The channel is on full video quality (`video_quality_mode` 2, J-11): the governor never
    /// reduces the layers handed to its members.
    pub video_full: bool,
}

pub struct JoinParams {
    pub user: UserId,
    pub guild: Option<GuildId>,
    pub channel: ChannelId,
    pub flags: Flags,
    /// The sender has completed the E2EE handshake already (tests; the gateway says so in production).
    pub e2ee_keyed: bool,
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
    #[error("bad video message: {0}")]
    Video(&'static str),
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
    /// The participant just started speaking (feeds the guild's AFK timer).
    Active { user: UserId, channel: ChannelId },
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
    /// Video frames forwarded to subscribers.
    pub forwarded_frames: u64,
    /// The egress governor's severity (0 = not engaged).
    pub governor_severity: u8,
    /// Keyframe requests sent to publishers.
    pub keyframe_requests: u64,
    /// Frames dropped because their sender had not completed the E2EE handshake (OPS-IMM-001).
    pub unkeyed_frames: u64,
    /// Music packets forwarded to subscribers (MUS-JB-001).
    pub music_packets: u64,
    /// The part of `egress_bps` that is music (a one-second window; it is already inside
    /// `egress_bps`, so the governor counts it once).
    pub music_egress_bps: f64,
}

/// A channel's music settings (MUS-JB-001), set by the gateway through [`SfuCore::set_music`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicRoom {
    /// `music_allowed`: when false the room's music lines are neither accepted nor forwarded.
    pub allowed: bool,
    /// `voice.music_bitrate_kbps` (32..=320). The SFU never decodes; this is informational.
    pub bitrate_kbps: u16,
}

impl Default for MusicRoom {
    fn default() -> Self {
        Self {
            allowed: true,
            bitrate_kbps: MUSIC_DEFAULT_KBPS,
        }
    }
}

/// What a remote m-line carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrackSource {
    Mic,
    Camera,
    Screen,
    ScreenAudio,
    /// A stereo music track (MUS-JB-001), a second audio line.
    Music,
}

impl TrackSource {
    pub const ALL: [TrackSource; 5] = [
        Self::Mic,
        Self::Camera,
        Self::Screen,
        Self::ScreenAudio,
        Self::Music,
    ];

    pub const fn kind(self) -> MediaKind {
        match self {
            Self::Camera | Self::Screen => MediaKind::Video,
            Self::Mic | Self::ScreenAudio | Self::Music => MediaKind::Audio,
        }
    }

    pub const fn from_proto(s: Source) -> Self {
        match s {
            Source::Camera => Self::Camera,
            Source::Screen => Self::Screen,
            Source::ScreenAudio => Self::ScreenAudio,
            Source::Music => Self::Music,
        }
    }

    pub const fn proto(self) -> Option<Source> {
        match self {
            Self::Mic => None,
            Self::Camera => Some(Source::Camera),
            Self::Screen => Some(Source::Screen),
            Self::ScreenAudio => Some(Source::ScreenAudio),
            Self::Music => Some(Source::Music),
        }
    }

    /// The msid stream id of the subscriber's line. The screen and its audio share one, so
    /// the browser plays them as one stream; `-` is a legal token character, `:` is not.
    pub fn stream_id(self, user: UserId) -> String {
        match self {
            Self::Mic => user.to_string(),
            Self::Camera => format!("{user}-camera"),
            Self::Screen | Self::ScreenAudio => format!("{user}-screen"),
            Self::Music => format!("{user}-music"),
        }
    }
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
    state: OutState,
    /// Video only: which simulcast layer flows.
    selector: Option<LayerSelector>,
}

#[derive(Default)]
struct Rates {
    last: Option<(Instant, u64, u64)>,
    rx_bps: f64,
    tx_bps: f64,
    loss: Option<f32>,
}

/// Bitrate and liveness of one simulcast layer of a published track.
#[derive(Debug, Clone, Copy, Default)]
struct LayerStat {
    window_start: Option<Instant>,
    bytes: u64,
    bps: f64,
    last_seen: Option<Instant>,
}

impl LayerStat {
    fn note(&mut self, now: Instant, len: usize) {
        self.last_seen = Some(now);
        let start = *self.window_start.get_or_insert(now);
        self.bytes += len as u64;
        let dt = now.saturating_duration_since(start);
        if dt >= Duration::from_secs(1) {
            self.bps = self.bytes as f64 * 8.0 / dt.as_secs_f64();
            self.bytes = 0;
            self.window_start = Some(now);
        }
    }

    /// The bitrate if frames arrived recently (`Some(0.0)` while the first window is open).
    fn live_bps(&self, now: Instant) -> Option<f64> {
        let seen = self.last_seen?;
        (now.saturating_duration_since(seen) <= LAYER_LIVE).then_some(self.bps)
    }
}

/// An inbound m-line of a participant.
struct Inbound {
    kind: MediaKind,
    /// The remote sends simulcast layers on it.
    simulcast: bool,
    /// The line is still in a receiving direction.
    receiving: bool,
    /// What it carries; `None` until the client says (video) or it is taken for the mic (audio).
    source: Option<TrackSource>,
    /// The client announced it as live.
    declared_active: bool,
    width: u32,
    height: u32,
    fps: u32,
    stats: [LayerStat; 3],
}

impl Inbound {
    fn available(&self, now: Instant) -> Available {
        let mut a = Available::default();
        for l in Layer::ALL {
            a.bps[l.index()] = self.stats[l.index()].live_bps(now);
        }
        a
    }
}

/// A client's `publish` announcement, kept until its m-line shows up (or after).
#[derive(Debug, Clone, Copy)]
struct Declared {
    source: TrackSource,
    active: bool,
    width: u32,
    height: u32,
    fps: u32,
}

struct Participant {
    user: UserId,
    guild: Option<GuildId>,
    channel: ChannelId,
    flags: Flags,
    /// Passed the E2EE handshake (OPS-IMM-001): nothing the participant sends is forwarded before.
    e2ee_keyed: bool,
    /// The channel allows music (`music_allowed`, MUS-JB-001); mirrors [`SfuCore::music_rooms`].
    music_allowed: bool,
    /// Mids of the client's own lines that listed the stereo Opus type (its music line): the SFU keeps
    /// listing only that type on them when it re-offers the session.
    stereo_mids: HashSet<String>,
    rtc: Rtc,
    /// Inbound m-lines by the client's mid.
    tracks: HashMap<Mid, Inbound>,
    declared: HashMap<Mid, Declared>,
    /// Which sources the other participants know are live (to announce changes once).
    announced: HashSet<TrackSource>,
    /// Subscriptions, keyed by the publisher and what they publish.
    out: HashMap<(ParticipantId, TrackSource), OutTrack>,
    /// What this participant asked for from each source (client hints).
    hints: HashMap<(UserId, Source), SubscriptionHint>,
    /// The bandwidth estimate towards this participant, bits per second (TWCC, when enabled).
    bwe_bps: Option<f64>,
    /// The loss-based budget, the fallback when the TWCC estimator is off.
    loss: LossBudget,
    /// The layer cap the governor currently sets for this participant.
    cap: Layer,
    /// The participant was told the room is busy.
    told_reduced: bool,
    /// A server offer waiting for the client's answer.
    pending: Option<SdpPendingOffer>,
    /// The client has sent at least one offer, so server offers are possible.
    negotiated: bool,
    /// Some line in `out` may be waiting to be offered or stopped. Lets the per-pump
    /// [`SfuCore::negotiate`] skip everyone else without scanning their lines.
    offer_due: bool,
    detector: crate::speaking::SpeakingDetector,
    /// Last push-to-talk state the client reported (`speaking` bitmask) and when it last dropped to 0.
    ptt_bits: u8,
    ptt_released_at: Option<Instant>,
    next_timeout: Option<Instant>,
    ice_down_since: Option<Instant>,
    dead: Option<CloseReason>,
    rates: Rates,
    /// The activity data channel (ACT-CH-001).
    dc: crate::datachannel::Member,
}

impl Participant {
    /// Whether audio from this participant is forwarded right now.
    fn may_send(&self, now: Instant) -> bool {
        if !self.e2ee_keyed || !self.flags.speak {
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

    /// The gateway lets this participant publish `source`.
    fn allowed(&self, source: TrackSource) -> bool {
        if !self.e2ee_keyed {
            return false;
        }
        match source {
            TrackSource::Mic => true,
            TrackSource::Camera => self.flags.camera,
            TrackSource::Screen | TrackSource::ScreenAudio => self.flags.screen,
            // SPEAK is required (a server-muted member cannot play music either) and the
            // channel must allow it.
            TrackSource::Music => self.flags.speak && self.music_allowed,
        }
    }

    /// The inbound line carrying `source` when it is live and allowed.
    fn live_track(&self, source: TrackSource) -> Option<(Mid, &Inbound)> {
        self.tracks
            .iter()
            .find(|(_, t)| {
                t.source == Some(source)
                    && t.receiving
                    && (source == TrackSource::Mic || t.declared_active)
                    && self.allowed(source)
            })
            .map(|(m, t)| (*m, t))
    }

    fn mic_mid(&self) -> Option<Mid> {
        self.tracks
            .iter()
            .find(|(_, t)| t.source == Some(TrackSource::Mic))
            .map(|(m, _)| *m)
    }

    fn video_outs(&self) -> usize {
        self.out
            .iter()
            .filter(|((_, s), t)| {
                s.kind() == MediaKind::Video
                    && matches!(t.state, OutState::Open(_) | OutState::Negotiating(_))
            })
            .count()
    }
}

pub struct SfuCore {
    parts: HashMap<ParticipantId, Participant>,
    rooms: HashMap<ChannelId, Vec<ParticipantId>>,
    by_user: HashMap<UserId, ParticipantId>,
    by_addr: HashMap<SocketAddr, ParticipantId>,
    out: VecDeque<CoreOutput>,
    next_id: u64,
    id_stride: u64,
    max_participants: usize,
    max_room_size: usize,
    forwarded: u64,
    forwarded_frames: u64,
    keyframe_requests: u64,
    unkeyed_frames: u64,
    /// Per-channel music settings that differ from the defaults (allowed, 256 kbps).
    music_rooms: HashMap<ChannelId, MusicRoom>,
    /// Bytes of music written to subscribers, for [`Snapshot::music_egress_bps`].
    music_tx: LayerStat,
    music_packets: u64,
    /// Who may receive each screen share: `(channel, owner) -> viewers`.
    acl: HashMap<(ChannelId, UserId), HashSet<UserId>>,
    /// Participants whose subscriptions must be re-derived before the next negotiation.
    dirty: HashSet<ParticipantId>,
    selector_cfg: SelectorConfig,
    /// str0m's TWCC estimator on every connection (see [`SfuCore::with_bwe`]).
    bwe: bool,
    keyframes: KeyframeCoalescer<(ParticipantId, TrackSource, Layer)>,
    governor: EgressGovernor,
    last_maintain: Option<Instant>,
    last_governor: Option<Instant>,
    /// `voice.datachannel_kbps`: each member's activity data budget.
    dc_kbps: u32,
    dc_stats: crate::datachannel::Stats,
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
            id_stride: 1,
            max_participants,
            max_room_size,
            forwarded: 0,
            forwarded_frames: 0,
            keyframe_requests: 0,
            unkeyed_frames: 0,
            music_rooms: HashMap::new(),
            music_tx: LayerStat::default(),
            music_packets: 0,
            acl: HashMap::new(),
            dirty: HashSet::new(),
            selector_cfg: SelectorConfig::default(),
            bwe: false,
            keyframes: KeyframeCoalescer::new(keyframe::Config::default()),
            governor: EgressGovernor::new(GovernorConfig::default()),
            last_maintain: None,
            last_governor: None,
            dc_kbps: crate::datachannel::DEFAULT_KBPS,
            dc_stats: crate::datachannel::Stats::default(),
        }
    }

    /// Each member's activity data-channel budget in kbit/s (`voice.datachannel_kbps`, ACT-CH-001); `0` turns the
    /// channel off. Applies to members who join afterwards.
    pub fn with_datachannel_kbps(mut self, kbps: u32) -> Self {
        self.dc_kbps = kbps;
        self
    }

    /// What the activity data channels have relayed and dropped so far.
    pub fn datachannel_stats(&self) -> crate::datachannel::Stats {
        self.dc_stats
    }

    /// Set the egress budget (VOX-SFU-008). A zero budget leaves the governor off.
    pub fn with_governor(mut self, cfg: GovernorConfig) -> Self {
        self.governor = EgressGovernor::new(cfg);
        self
    }

    /// Hand out participant ids `shard + 1, shard + 1 + shards, ...` so that cores running
    /// side by side (VOX-SFU-011 sharding) never collide and the owner of an id is
    /// `(id - 1) % shards`.
    pub fn with_id_space(mut self, shard: u64, shards: u64) -> Self {
        let shards = shards.max(1);
        self.next_id = shard % shards + 1;
        self.id_stride = shards;
        self
    }

    /// Turn on str0m's TWCC bandwidth estimator for every connection. It gives the layer
    /// selector a real per-subscriber estimate, but str0m then pads each sending connection
    /// to 50 kbit/s to keep the estimate alive, audio-only ones included, so it is off by
    /// default; the loss-based fallback ([`crate::loss`]) is used instead.
    pub fn with_bwe(mut self, on: bool) -> Self {
        self.bwe = on;
        self
    }

    pub fn with_selector(mut self, cfg: SelectorConfig) -> Self {
        self.selector_cfg = cfg;
        self
    }

    pub fn with_keyframes(mut self, cfg: keyframe::Config) -> Self {
        self.keyframes = KeyframeCoalescer::new(cfg);
        self
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

    /// The layer currently forwarded from `publisher`'s `source` to `subscriber`, if any.
    pub fn layer_of(
        &self,
        subscriber: ParticipantId,
        publisher: ParticipantId,
        source: TrackSource,
    ) -> Option<Layer> {
        self.parts
            .get(&subscriber)?
            .out
            .get(&(publisher, source))?
            .selector
            .as_ref()?
            .current()
    }

    /// Whether `subscriber` has an open (or opening) line for `publisher`'s `source`.
    pub fn subscribes(
        &self,
        subscriber: ParticipantId,
        publisher: ParticipantId,
        source: TrackSource,
    ) -> bool {
        self.parts.get(&subscriber).is_some_and(|p| {
            p.out.get(&(publisher, source)).is_some_and(|t| {
                !matches!(t.state, OutState::ToStop(_) | OutState::NegotiatingStop(_))
            })
        })
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
            forwarded_frames: self.forwarded_frames,
            governor_severity: self.governor.severity(),
            keyframe_requests: self.keyframe_requests,
            unkeyed_frames: self.unkeyed_frames,
            music_packets: self.music_packets,
            music_egress_bps: self
                .last_maintain
                .and_then(|now| self.music_tx.live_bps(now))
                .unwrap_or(0.0),
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
        let maintain = self
            .parts
            .values()
            .any(|p| !p.out.is_empty() || !p.tracks.is_empty())
            .then(|| {
                self.last_maintain
                    .map_or_else(Instant::now, |t| t + MAINTAIN_EVERY)
            });
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
            .chain(maintain)
            .chain(self.keyframes.next_deadline())
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
        let mut builder = Rtc::builder()
            .set_ice_lite(true)
            // Opus for voice; VP8 and H.264 (with RTX, so NACKs are answered from the send
            // buffer) for camera and screen.
            .clear_codecs()
            .enable_vp8(true)
            .enable_h264(true)
            .set_send_buffer_video(1_000)
            .set_stats_interval(Some(Duration::from_secs(1)));
        // Two Opus payload types (MUS-JB-001): 111 is mono (the microphone and screen audio, no `stereo`
        // in its fmtp), 112 is stereo (`stereo=1; sprop-stereo=1`, the music track). Which audio line
        // carries which is fixed in the SDP: `restrict_opus_lines` is applied to every offer the SFU makes.
        for (pt, stereo) in [(MONO_OPUS_PT, false), (STEREO_OPUS_PT, true)] {
            builder.codec_config().add_config(
                Pt::new_with_value(pt),
                None,
                Codec::Opus,
                Frequency::FORTY_EIGHT_KHZ,
                Some(2),
                FormatParams {
                    min_p_time: Some(10),
                    use_inband_fec: Some(true),
                    stereo: stereo.then_some(true),
                    sprop_stereo: stereo.then_some(true),
                    ..Default::default()
                },
            );
        }
        if self.bwe {
            // TWCC bandwidth estimation per subscriber drives layer selection.
            builder = builder.enable_bwe(Some(INITIAL_BWE));
        }
        let mut rtc = builder.build(now);
        for c in p.candidates {
            rtc.add_local_candidate(c);
        }
        let pid = ParticipantId(self.next_id);
        self.next_id += self.id_stride;
        let room = self.rooms.entry(p.channel).or_default();
        let existing = room.clone();
        room.push(pid);
        self.by_user.insert(p.user, pid);
        let cap = cap_of(self.governor.severity(), &p.flags);
        self.parts.insert(
            pid,
            Participant {
                user: p.user,
                guild: p.guild,
                channel: p.channel,
                flags: p.flags,
                e2ee_keyed: p.e2ee_keyed,
                stereo_mids: HashSet::new(),
                music_allowed: self.music_rooms.get(&p.channel).is_none_or(|m| m.allowed),
                rtc,
                tracks: HashMap::new(),
                declared: HashMap::new(),
                announced: HashSet::new(),
                out: HashMap::new(),
                hints: HashMap::new(),
                bwe_bps: None,
                loss: LossBudget::default(),
                cap,
                told_reduced: false,
                pending: None,
                negotiated: false,
                offer_due: false,
                detector: crate::speaking::SpeakingDetector::default(),
                ptt_bits: 0,
                ptt_released_at: None,
                next_timeout: None,
                ice_down_since: None,
                dead: None,
                rates: Rates::default(),
                dc: crate::datachannel::Member::new(self.dc_kbps),
            },
        );
        // Existing participants' tracks become this one's subscriptions, and it learns what
        // they publish.
        for other in existing {
            self.signal(other, op::CLIENT_CONNECT, &UserRef { user_id: p.user });
            self.dirty.insert(other);
            for msg in self.published_messages(other) {
                self.signal(pid, op::VIDEO, &msg);
            }
        }
        self.dirty.insert(pid);
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
        self.acl.retain(|(c, _), _| *c != channel);
        self.music_rooms.remove(&channel);
    }

    /// Apply a channel's music settings (MUS-JB-001). Takes effect at once for the members
    /// in the room; a room that opens later starts with them. Turning music off stops the
    /// forwarding and withdraws the lines on the next negotiation. Returns whether the
    /// settings differ from the defaults (and so are kept).
    pub fn set_music(
        &mut self,
        now: Instant,
        channel: ChannelId,
        allowed: bool,
        kbps: u16,
    ) -> bool {
        let room = MusicRoom {
            allowed,
            bitrate_kbps: clamp_music_kbps(kbps),
        };
        let kept = room != MusicRoom::default();
        if kept {
            self.music_rooms.insert(channel, room);
        } else {
            self.music_rooms.remove(&channel);
        }
        let members = self.rooms.get(&channel).cloned().unwrap_or_default();
        for pid in &members {
            if let Some(p) = self.parts.get_mut(pid) {
                p.music_allowed = allowed;
            }
            self.refresh_published(*pid);
        }
        self.mark_room_dirty(channel);
        self.pump(now);
        kept
    }

    /// The music settings of a channel (the defaults when none were set).
    pub fn music_of(&self, channel: ChannelId) -> MusicRoom {
        self.music_rooms.get(&channel).copied().unwrap_or_default()
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
        self.dirty.remove(&pid);
        self.keyframes.forget_where(|(who, _, _)| *who == pid);
        self.acl.remove(&(p.channel, p.user));
        for viewers in self.acl.values_mut() {
            viewers.remove(&p.user);
        }
        if let Some(room) = self.rooms.get_mut(&p.channel) {
            room.retain(|x| *x != pid);
            if room.is_empty() {
                self.rooms.remove(&p.channel);
            }
        }
        let members = self.rooms.get(&p.channel).cloned().unwrap_or_default();
        for m in members {
            // The speaker's lines are stopped by the next negotiation: `reconcile` sees the
            // publisher is gone.
            self.dirty.insert(m);
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
            for source in [
                Source::Camera,
                Source::Screen,
                Source::ScreenAudio,
                Source::Music,
            ] {
                if p.announced.contains(&TrackSource::from_proto(source)) {
                    self.signal(
                        m,
                        op::VIDEO,
                        &Video::Published {
                            user_id: p.user,
                            source,
                            active: false,
                            width: 0,
                            height: 0,
                            fps: 0,
                        },
                    );
                }
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
        // str0m matches its two Opus entries against what each line lists, so it must see both on every audio
        // line; the lines that listed the stereo type alone carry music, and the answer goes back narrowed.
        let (wide, music_mids) = expand_opus_lines(sdp);
        let offer = SdpOffer::from_sdp_string(&wide).map_err(|e| SfuError::Sdp(e.to_string()))?;
        let p = self
            .parts
            .get_mut(&pid)
            .ok_or(SfuError::UnknownParticipant)?;
        p.stereo_mids.extend(music_mids.iter().cloned());
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
            p.offer_due = true;
        }
        let answer = p
            .rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| SfuError::Rtc(e.to_string()))?;
        p.negotiated = true;
        self.dirty.insert(pid);
        self.pump(now);
        Ok(restrict_opus_lines(&answer.to_sdp_string(), &music_mids))
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
        // The answer lists one Opus type per line (we offered it narrowed): widen it for str0m.
        let (wide, _) = expand_opus_lines(sdp);
        let answer = SdpAnswer::from_sdp_string(&wide).map_err(|e| SfuError::Sdp(e.to_string()))?;
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
        // Lines opened by this answer may already be unwanted again, or wanted ones missing.
        self.dirty.insert(pid);
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
            p.offer_due = true;
        }
    }

    /// Override the bandwidth estimate for `pid` (bits per second), as the TWCC estimator
    /// does when it is on. `None` clears it. Used by tests and by other estimators.
    pub fn set_bandwidth_estimate(&mut self, pid: ParticipantId, bps: Option<f64>) {
        if let Some(p) = self.parts.get_mut(&pid) {
            p.bwe_bps = bps;
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

    /// A client's `VIDEO` message (op 12): declare a track or change subscriptions.
    pub fn handle_video(
        &mut self,
        now: Instant,
        pid: ParticipantId,
        msg: Video,
    ) -> Result<(), SfuError> {
        match msg {
            Video::Publish {
                mid,
                source,
                active,
                width,
                height,
                fps,
            } => {
                let source = TrackSource::from_proto(source);
                let mid = Mid::from(mid.as_str());
                let p = self
                    .parts
                    .get_mut(&pid)
                    .ok_or(SfuError::UnknownParticipant)?;
                if let Some(t) = p.tracks.get(&mid)
                    && t.kind != source.kind()
                {
                    return Err(SfuError::Video("source does not match the m-line kind"));
                }
                // One m-line per source.
                if p.declared
                    .iter()
                    .any(|(m, d)| *m != mid && d.source == source && d.active && active)
                {
                    return Err(SfuError::Video("that source is already published"));
                }
                p.declared.insert(
                    mid,
                    Declared {
                        source,
                        active,
                        width,
                        height,
                        fps,
                    },
                );
                self.apply_declaration(pid, mid);
            }
            Video::Subscribe { subscriptions } => {
                let p = self
                    .parts
                    .get_mut(&pid)
                    .ok_or(SfuError::UnknownParticipant)?;
                for h in subscriptions {
                    p.hints.insert((h.user_id, h.source), h);
                }
                self.dirty.insert(pid);
                self.apply_hints(pid);
            }
            // Server-to-client messages are not accepted from clients.
            Video::Published { .. } | Video::Layer { .. } => {
                return Err(SfuError::Video("server message from a client"));
            }
        }
        self.pump(now);
        Ok(())
    }

    /// The gateway lists who may receive `owner`'s screen share in `channel`. The SFU sends
    /// the screen (and its audio) to nobody else; `live: false` ends it.
    pub fn set_stream_viewers(
        &mut self,
        now: Instant,
        channel: ChannelId,
        owner: UserId,
        viewers: Vec<UserId>,
        live: bool,
    ) {
        if live {
            self.acl
                .insert((channel, owner), viewers.into_iter().collect());
        } else {
            self.acl.remove(&(channel, owner));
        }
        self.mark_room_dirty(channel);
        self.pump(now);
    }

    /// Apply new media-path flags (server mute, deafen, permission change, sender allowance).
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
        let publish_changed = p.flags.camera != flags.camera || p.flags.screen != flags.screen;
        p.flags = flags;
        // A speaker who may no longer send stops speaking for everyone at once.
        if !p.may_send(now) && p.detector.reset() && was_speaking {
            self.announce_speaking(pid, false, None);
        }
        if publish_changed {
            self.refresh_published(pid);
        }
        self.pump(now);
        true
    }

    /// Record that `user` completed (or lost) the E2EE handshake. Returns whether `user` is in `channel`.
    pub fn set_e2ee_keyed(
        &mut self,
        now: Instant,
        channel: ChannelId,
        user: UserId,
        keyed: bool,
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
        let changed = p.e2ee_keyed != keyed;
        p.e2ee_keyed = keyed;
        if changed {
            // What they publish becomes visible (or hidden) to the others, and a speaker who may no
            // longer send stops speaking for everyone at once.
            let was_speaking = p.detector.is_speaking();
            if !keyed && p.detector.reset() && was_speaking {
                self.announce_speaking(pid, false, None);
            }
            self.refresh_published(pid);
            // Their microphone joins (or leaves) everyone else's subscriptions.
            self.mark_room_dirty(channel);
        }
        self.pump(now);
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
        let taken = self.queue_input(now, proto, source, destination, data);
        if taken {
            self.pump(now);
        }
        taken
    }

    /// Hand a datagram to its participant without running the room yet; [`Self::flush`] does that
    /// once for a whole batch. Every pump visits every participant, so pumping after each packet of
    /// a batch made a room's per-second cost grow with the cube of its size (RDY-VOX-002).
    pub fn queue_input(
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
        true
    }

    /// Run the room after [`Self::queue_input`]: forward what arrived, negotiate, drop the dead.
    pub fn flush(&mut self, now: Instant) {
        self.pump(now);
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
        if self
            .last_maintain
            .is_none_or(|t| now.saturating_duration_since(t) >= MAINTAIN_EVERY)
        {
            self.last_maintain = Some(now);
            self.maintain(now);
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

    fn mark_room_dirty(&mut self, channel: ChannelId) {
        if let Some(room) = self.rooms.get(&channel) {
            self.dirty.extend(room.iter().copied());
        }
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
        let (user, channel) = (p.user, p.channel);
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
        if on {
            self.out
                .push_back(CoreOutput::Event(CoreEvent::Active { user, channel }));
        }
    }

    /// The `Published` messages describing `pid`'s live camera and screen tracks.
    fn published_messages(&self, pid: ParticipantId) -> Vec<Video> {
        let Some(p) = self.parts.get(&pid) else {
            return Vec::new();
        };
        [
            TrackSource::Camera,
            TrackSource::Screen,
            TrackSource::ScreenAudio,
            TrackSource::Music,
        ]
        .into_iter()
        .filter_map(|s| {
            let (_, t) = p.live_track(s)?;
            Some(Video::Published {
                user_id: p.user,
                source: s.proto()?,
                active: true,
                width: t.width,
                height: t.height,
                fps: t.fps,
            })
        })
        .collect()
    }

    /// Re-check which of `pid`'s tracks are live and tell the room about changes.
    fn refresh_published(&mut self, pid: ParticipantId) {
        let Some(p) = self.parts.get_mut(&pid) else {
            return;
        };
        let mut changes: Vec<Video> = Vec::new();
        for s in [
            TrackSource::Camera,
            TrackSource::Screen,
            TrackSource::ScreenAudio,
            TrackSource::Music,
        ] {
            let live = p.live_track(s).map(|(_, t)| (t.width, t.height, t.fps));
            let known = p.announced.contains(&s);
            let Some(proto) = s.proto() else { continue };
            match (live, known) {
                (Some((width, height, fps)), false) => {
                    p.announced.insert(s);
                    changes.push(Video::Published {
                        user_id: p.user,
                        source: proto,
                        active: true,
                        width,
                        height,
                        fps,
                    });
                }
                (None, true) => {
                    p.announced.remove(&s);
                    changes.push(Video::Published {
                        user_id: p.user,
                        source: proto,
                        active: false,
                        width: 0,
                        height: 0,
                        fps: 0,
                    });
                }
                _ => {}
            }
        }
        if changes.is_empty() {
            return;
        }
        let (channel, me) = (p.channel, pid);
        let audience: Vec<_> = self
            .rooms
            .get(&channel)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| *id != me)
            .collect();
        for to in audience {
            for c in &changes {
                self.signal(to, op::VIDEO, c);
            }
        }
        self.mark_room_dirty(channel);
    }

    /// Bind a declaration to its inbound line (when both are known) and refresh.
    fn apply_declaration(&mut self, pid: ParticipantId, mid: Mid) {
        if let Some(p) = self.parts.get_mut(&pid)
            && let (Some(d), Some(t)) = (p.declared.get(&mid).copied(), p.tracks.get_mut(&mid))
            && t.kind == d.source.kind()
        {
            t.source = Some(d.source);
            t.declared_active = d.active;
            t.width = d.width;
            t.height = d.height;
            t.fps = d.fps;
        }
        self.refresh_published(pid);
    }

    /// Recompute each video subscription's hint cap from the client's hints.
    fn apply_hints(&mut self, id: ParticipantId) {
        let users: HashMap<ParticipantId, (UserId, [Option<u32>; 5])> = self
            .parts
            .iter()
            .map(|(k, v)| {
                let mut heights = [None; 5];
                for s in TrackSource::ALL {
                    heights[s as usize] = v.live_track(s).map(|(_, t)| t.height);
                }
                (*k, (v.user, heights))
            })
            .collect();
        let Some(q) = self.parts.get_mut(&id) else {
            return;
        };
        for ((pubid, src), t) in &mut q.out {
            let (Some(sel), Some((user, heights))) = (t.selector.as_mut(), users.get(pubid)) else {
                continue;
            };
            let Some(proto) = src.proto() else { continue };
            let hint = q.hints.get(&(*user, proto));
            let full = heights[*src as usize].unwrap_or(0);
            sel.set_hint_cap(hint_cap(hint, full, *src));
        }
    }

    /// Re-derive `id`'s subscriptions from who publishes what, the client's hints and the
    /// viewer lists. This is the single place that decides which lines a subscriber has.
    fn reconcile(&mut self, id: ParticipantId) {
        let Some(q) = self.parts.get(&id) else { return };
        let (channel, my_user) = (q.channel, q.user);
        let room = self.rooms.get(&channel).cloned().unwrap_or_default();
        let mut want: HashSet<(ParticipantId, TrackSource)> = HashSet::new();
        for o in room {
            if o == id {
                continue;
            }
            let Some(p) = self.parts.get(&o) else {
                continue;
            };
            let enabled =
                |s: Source, default: bool| q.hints.get(&(p.user, s)).map_or(default, |h| h.enabled);
            if p.live_track(TrackSource::Mic).is_some() {
                want.insert((o, TrackSource::Mic));
            }
            if p.live_track(TrackSource::Camera).is_some() && enabled(Source::Camera, true) {
                want.insert((o, TrackSource::Camera));
            }
            // Music: everyone gets it unless they muted it (a hint with `enabled: false`).
            if p.live_track(TrackSource::Music).is_some() && enabled(Source::Music, true) {
                want.insert((o, TrackSource::Music));
            }
            let may_watch = self
                .acl
                .get(&(channel, p.user))
                .is_some_and(|v| v.contains(&my_user));
            if p.live_track(TrackSource::Screen).is_some()
                && may_watch
                && enabled(Source::Screen, false)
            {
                want.insert((o, TrackSource::Screen));
                if p.live_track(TrackSource::ScreenAudio).is_some() {
                    want.insert((o, TrackSource::ScreenAudio));
                }
            }
        }
        let cfg = self.selector_cfg;
        let cap = self.parts.get(&id).map_or(Layer::High, |p| p.cap);
        let Some(q) = self.parts.get_mut(&id) else {
            return;
        };
        for key in &want {
            q.out.entry(*key).or_insert_with(|| {
                let selector = (key.1.kind() == MediaKind::Video).then(|| {
                    let mut s = LayerSelector::new(cfg);
                    s.set_governor_cap(cap);
                    s
                });
                OutTrack {
                    state: OutState::ToOpen,
                    selector,
                }
            });
        }
        let unwanted: Vec<_> = q
            .out
            .keys()
            .filter(|k| !want.contains(k))
            .copied()
            .collect();
        for key in unwanted {
            let Some(t) = q.out.get_mut(&key) else {
                continue;
            };
            match t.state {
                OutState::ToOpen => {
                    q.out.remove(&key);
                }
                OutState::Open(m) => t.state = OutState::ToStop(m),
                // Mid-negotiation lines settle first; the answer marks `id` dirty again.
                _ => {}
            }
        }
        q.offer_due = true;
        self.apply_hints(id);
    }

    /// Once per [`MAINTAIN_EVERY`]: governor, layer selection, keyframe timers, notices.
    fn maintain(&mut self, now: Instant) {
        // Governor: node egress against the budget, evaluated about once a second.
        if self
            .last_governor
            .is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_secs(1))
        {
            self.last_governor = Some(now);
            let egress: f64 = self.parts.values().map(|p| p.rates.tx_bps).sum();
            self.governor.update(now, egress);
        }
        let severity = self.governor.severity();
        let ids: Vec<ParticipantId> = self.parts.keys().copied().collect();
        // Publishers' availability, gathered first (the loop below borrows subscribers mutably).
        type Key = (ParticipantId, TrackSource);
        let avail: HashMap<Key, (Available, u32)> = self
            .parts
            .iter()
            .flat_map(|(pid, p)| {
                p.tracks.values().filter_map(move |t| {
                    let s = t.source.filter(|s| s.kind() == MediaKind::Video)?;
                    Some(((*pid, s), (t.available(now), t.height)))
                })
            })
            .collect();
        let mut requests: Vec<(ParticipantId, TrackSource, Layer)> = Vec::new();
        for id in ids {
            let Some(q) = self.parts.get_mut(&id) else {
                continue;
            };
            let cap = cap_of(severity, &q.flags);
            q.cap = cap;
            let n_video = q.video_outs().max(1);
            let budget = match (q.bwe_bps, q.loss.budget()) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let share = budget.map(|b| b / n_video as f64);
            let mut any_video = false;
            for ((pubid, src), t) in &mut q.out {
                let Some(sel) = t.selector.as_mut() else {
                    continue;
                };
                any_video = true;
                sel.set_governor_cap(cap);
                let Some((a, _)) = avail.get(&(*pubid, *src)) else {
                    continue;
                };
                if let Some(layer) = sel.update(now, a, share) {
                    requests.push((*pubid, *src, layer));
                }
            }
            // QUALITY notices when the cap bites or lifts for someone who receives video.
            let reduced_now = any_video && cap != Layer::High;
            if reduced_now != q.told_reduced {
                q.told_reduced = reduced_now;
                let msg = notice(cap);
                self.signal(id, op::QUALITY, &msg);
            }
        }
        for (pubid, src, layer) in requests {
            self.request_keyframe(now, pubid, src, layer, keyframe::Kind::Pli);
        }
        for ((pubid, src, layer), kind) in self.keyframes.poll(now) {
            self.send_keyframe_request(pubid, src, layer, kind);
        }
    }

    /// Ask `publisher` for a keyframe of `layer`, coalesced with everyone else's asks.
    fn request_keyframe(
        &mut self,
        now: Instant,
        publisher: ParticipantId,
        source: TrackSource,
        layer: Layer,
        kind: keyframe::Kind,
    ) {
        if let Some(kind) = self
            .keyframes
            .request((publisher, source, layer), kind, now)
        {
            self.send_keyframe_request(publisher, source, layer, kind);
        }
    }

    fn send_keyframe_request(
        &mut self,
        publisher: ParticipantId,
        source: TrackSource,
        layer: Layer,
        kind: keyframe::Kind,
    ) {
        let Some(p) = self.parts.get_mut(&publisher) else {
            return;
        };
        let Some((mid, simulcast)) = p.live_track(source).map(|(m, t)| (m, t.simulcast)) else {
            return;
        };
        let rid = simulcast.then(|| Rid::from(layer.rid()));
        let kind = match kind {
            keyframe::Kind::Pli => KeyframeRequestKind::Pli,
            keyframe::Kind::Fir => KeyframeRequestKind::Fir,
        };
        if let Some(mut w) = p.rtc.writer(mid)
            && w.request_keyframe(rid, kind).is_ok()
        {
            self.keyframe_requests += 1;
        }
    }

    /// Drive every Rtc until nothing more happens, forwarding as media arrives.
    fn pump(&mut self, now: Instant) {
        // Bounded: forwarding feeds other Rtcs, which can then have output of their own.
        for _ in 0..32 {
            let mut work = false;
            // Subscriptions that must be re-derived come first, so one offer covers them all.
            let dirty: Vec<_> = self.dirty.drain().collect();
            for id in dirty {
                self.reconcile(id);
            }
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
            if !self.dirty.is_empty() {
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
                tracing::debug!(pid = id.0, state = ?s, "voice ICE state");
                if let Some(p) = self.parts.get_mut(&id) {
                    p.ice_down_since = match s {
                        IceConnectionState::Disconnected => p.ice_down_since.or(Some(now)),
                        _ => None,
                    };
                }
            }
            Event::MediaAdded(m) => {
                if !m.direction.is_receiving() {
                    return;
                }
                let Some(p) = self.parts.get_mut(&id) else {
                    return;
                };
                // What it carries: the client's declaration, or (audio) the microphone.
                let declared = p.declared.get(&m.mid).copied();
                let has_mic = p.mic_mid().is_some();
                let source = match (declared, m.kind) {
                    (Some(d), _) if d.source.kind() == m.kind => Some(d.source),
                    (_, MediaKind::Audio) if !has_mic => Some(TrackSource::Mic),
                    _ => None,
                };
                p.tracks.insert(
                    m.mid,
                    Inbound {
                        kind: m.kind,
                        simulcast: m.simulcast.is_some(),
                        receiving: true,
                        source,
                        declared_active: declared.is_some_and(|d| d.active),
                        width: declared.map_or(0, |d| d.width),
                        height: declared.map_or(0, |d| d.height),
                        fps: declared.map_or(0, |d| d.fps),
                        stats: [LayerStat::default(); 3],
                    },
                );
                let channel = p.channel;
                self.refresh_published(id);
                self.mark_room_dirty(channel);
            }
            Event::MediaChanged(m) => {
                let Some(p) = self.parts.get_mut(&id) else {
                    return;
                };
                if let Some(t) = p.tracks.get_mut(&m.mid) {
                    t.receiving = m.direction.is_receiving();
                }
                let channel = p.channel;
                self.refresh_published(id);
                self.mark_room_dirty(channel);
            }
            Event::MediaData(d) => {
                let kind = self
                    .parts
                    .get(&id)
                    .and_then(|p| p.tracks.get(&d.mid))
                    .and_then(|t| t.source);
                match kind {
                    Some(TrackSource::Mic) => self.forward_audio(id, TrackSource::Mic, d, now),
                    Some(TrackSource::ScreenAudio) => {
                        self.forward_audio(id, TrackSource::ScreenAudio, d, now);
                    }
                    Some(TrackSource::Music) => self.forward_audio(id, TrackSource::Music, d, now),
                    Some(s @ (TrackSource::Camera | TrackSource::Screen)) => {
                        self.forward_video(id, s, &d, now);
                    }
                    None => {}
                }
            }
            Event::KeyframeRequest(k) => {
                // A subscriber lost the picture on one of its lines: find whose it is.
                let Some(q) = self.parts.get(&id) else { return };
                let hit = q.out.iter().find_map(|((pubid, src), t)| {
                    (t.state.mid() == Some(k.mid) && src.kind() == MediaKind::Video).then(|| {
                        (
                            *pubid,
                            *src,
                            t.selector.as_ref().and_then(LayerSelector::current),
                        )
                    })
                });
                if let Some((pubid, src, layer)) = hit {
                    let layer = layer.unwrap_or(Layer::Low);
                    let kind = match k.kind {
                        KeyframeRequestKind::Pli => keyframe::Kind::Pli,
                        KeyframeRequestKind::Fir => keyframe::Kind::Fir,
                    };
                    self.request_keyframe(now, pubid, src, layer, kind);
                }
            }
            Event::EgressBitrateEstimate(kind) => {
                let est = match kind {
                    str0m::bwe::BweKind::Twcc { estimate, .. }
                    | str0m::bwe::BweKind::Remb { estimate, .. } => estimate.as_f64(),
                    _ => return,
                };
                if let Some(p) = self.parts.get_mut(&id) {
                    p.bwe_bps = Some(est);
                }
            }
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
                    if let Some(l) = s.egress_loss_fraction {
                        p.loss.on_report(now, l, p.rates.tx_bps);
                    }
                }
            }
            Event::ChannelOpen(cid, label) => {
                // One activity data channel per participant (ACT-CH-001); any other label is ignored.
                if label == crate::datachannel::LABEL
                    && let Some(p) = self.parts.get_mut(&id)
                    && p.dc.channel.is_none()
                {
                    p.dc.channel = Some(cid);
                }
            }
            Event::ChannelClose(cid) => {
                if let Some(p) = self.parts.get_mut(&id)
                    && p.dc.channel == Some(cid)
                {
                    p.dc.channel = None;
                    p.dc.instances.clear();
                }
            }
            Event::ChannelData(cd) => self.on_channel_data(id, cd, now),
            Event::Connected => {
                tracing::debug!(pid = id.0, "voice DTLS connected");
            }
            Event::Closed => {
                tracing::debug!(pid = id.0, "voice peer connection closed");
                if let Some(p) = self.parts.get_mut(&id) {
                    p.dead = Some(CloseReason::Connection);
                }
            }
            _ => {}
        }
    }

    /// A message on a participant's activity data channel (ACT-CH-001): join or leave an instance, or relay
    /// one opaque message to the other members of the same room that joined the same instance.
    fn on_channel_data(
        &mut self,
        src: ParticipantId,
        cd: str0m::channel::ChannelData,
        now: Instant,
    ) {
        use crate::datachannel::{self as dc, Frame};
        let Some(p) = self.parts.get_mut(&src) else {
            return;
        };
        // Only the participant's own activity channel counts, and only binary frames.
        if p.dc.channel != Some(cd.id) {
            return;
        }
        if !cd.binary {
            self.dc_stats.dropped_scope += 1;
            return;
        }
        // The budget is charged for every frame a member sends, valid or not.
        if !p.dc.budget.allow(now, cd.data.len()) {
            self.dc_stats.dropped_budget += 1;
            return;
        }
        match dc::parse(&cd.data) {
            Some(Frame::Join { instance }) => {
                if !p.dc.join(instance) {
                    self.dc_stats.dropped_scope += 1;
                }
            }
            Some(Frame::Leave { instance }) => {
                p.dc.instances.remove(&instance);
            }
            Some(Frame::Data { instance, payload }) => {
                // Nothing before the E2EE handshake (D-249), and only into an instance the sender joined.
                if !p.e2ee_keyed || !p.dc.instances.contains(&instance) {
                    self.dc_stats.dropped_scope += 1;
                    return;
                }
                let (room, user) = (p.channel, p.user);
                let frame = dc::relayed(instance, user, payload);
                let targets: Vec<ParticipantId> = self
                    .rooms
                    .get(&room)
                    .map(|m| m.iter().copied().filter(|x| *x != src).collect())
                    .unwrap_or_default();
                for t in targets {
                    let Some(q) = self.parts.get_mut(&t) else {
                        continue;
                    };
                    if !q.e2ee_keyed || !q.dc.instances.contains(&instance) {
                        continue;
                    }
                    let Some(ch) = q.dc.channel else { continue };
                    match q.rtc.channel(ch).map(|mut c| c.write(true, &frame)) {
                        Some(Ok(true)) => {
                            self.dc_stats.relayed += 1;
                            self.dc_stats.relayed_bytes += frame.len() as u64;
                        }
                        _ => self.dc_stats.dropped_backpressure += 1,
                    }
                }
            }
            None => self.dc_stats.dropped_scope += 1,
        }
    }

    /// Forward one audio frame (microphone, screen audio or music) to the room.
    fn forward_audio(
        &mut self,
        src: ParticipantId,
        source: TrackSource,
        d: MediaData,
        now: Instant,
    ) {
        let Some(p) = self.parts.get_mut(&src) else {
            return;
        };
        let Some(track) = p.tracks.get(&d.mid) else {
            return;
        };
        if !p.e2ee_keyed {
            self.unkeyed_frames += 1;
            return;
        }
        let allowed = match source {
            TrackSource::Mic => p.may_send(now),
            _ => track.declared_active && track.receiving && p.allowed(source),
        };
        if !allowed {
            return;
        }
        let channel = p.channel;
        let level = d.ext_vals.audio_level;
        let vad = d.ext_vals.voice_activity;
        if source == TrackSource::Mic {
            let transition = p.detector.on_packet(now, level, vad);
            if let Some(on) = transition {
                self.announce_speaking(src, on, level);
            }
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
            let Some(mid) = q.out.get(&(src, source)).and_then(|t| match t.state {
                OutState::Open(m) => Some(m),
                _ => None,
            }) else {
                continue;
            };
            let Some(mut w) = q.rtc.writer(mid) else {
                continue;
            };
            // Pass the level on so receiving clients can draw meters without decoding.
            if source == TrackSource::Mic
                && let (Some(l), v) = (level, vad)
            {
                w = w.audio_level(l, v.unwrap_or(true));
            }
            let Some(pt) = w.match_params(d.params) else {
                continue;
            };
            if w.write(pt, d.network_time, d.time, d.data.clone()).is_ok() {
                self.forwarded += 1;
                if source == TrackSource::Music {
                    self.music_packets += 1;
                    self.music_tx.note(now, d.data.len());
                }
            }
        }
    }

    /// Forward one video frame (camera or screen) to the subscribers whose selector wants
    /// this layer.
    fn forward_video(
        &mut self,
        src: ParticipantId,
        source: TrackSource,
        d: &MediaData,
        now: Instant,
    ) {
        let layer = d
            .rid
            .and_then(|r| Layer::from_rid(&r))
            // A line without simulcast is one full-size layer.
            .unwrap_or(Layer::High);
        let keyframe = d.is_keyframe();
        let Some(p) = self.parts.get_mut(&src) else {
            return;
        };
        if !p.e2ee_keyed {
            self.unkeyed_frames += 1;
            return;
        }
        let channel = p.channel;
        let user = p.user;
        let live = p.allowed(source);
        let Some(track) = p.tracks.get_mut(&d.mid) else {
            return;
        };
        if !live || !track.declared_active || !track.receiving {
            return;
        }
        track.stats[layer.index()].note(now, d.data.len());
        if keyframe {
            self.keyframes.keyframe_seen((src, source, layer));
        }
        let targets: Vec<_> = self
            .rooms
            .get(&channel)
            .into_iter()
            .flatten()
            .copied()
            .filter(|o| *o != src)
            .collect();
        let mut switched: Vec<(ParticipantId, Layer)> = Vec::new();
        for o in targets {
            let Some(q) = self.parts.get_mut(&o) else {
                continue;
            };
            let Some(t) = q.out.get_mut(&(src, source)) else {
                continue;
            };
            let OutState::Open(mid) = t.state else {
                continue;
            };
            let Some(sel) = t.selector.as_mut() else {
                continue;
            };
            // A source with only one layer (a screen share) needs no selection.
            if sel.current().is_none() && sel.pending().is_none() && !d.rid.is_some() {
                sel.force_single(layer);
            }
            let decision = sel.on_frame(layer, keyframe);
            match decision {
                Frame::Drop => continue,
                Frame::Switched(l) => switched.push((o, l)),
                Frame::Forward => {}
            }
            let Some(w) = q.rtc.writer(mid) else {
                continue;
            };
            let Some(pt) = w.match_params(d.params) else {
                continue;
            };
            if w.write(pt, d.network_time, d.time, d.data.clone()).is_ok() {
                self.forwarded_frames += 1;
            }
        }
        for (o, l) in switched {
            if let Some(proto) = source.proto() {
                self.signal(
                    o,
                    op::VIDEO,
                    &Video::Layer {
                        user_id: user,
                        source: proto,
                        layer: l,
                        reason: sr_voice_proto::video::LayerReason::Hint,
                    },
                );
            }
        }
    }

    /// Start a server offer for `id` if its subscriptions changed. Returns whether it did.
    fn negotiate(&mut self, id: ParticipantId) -> bool {
        // Called for every participant on every pump: decide in O(1) that there is nothing to do. It
        // used to build a map of the room and scan every line first, so each pump cost O(room size
        // squared) (RDY-VOX-002).
        let Some(p) = self.parts.get_mut(&id) else {
            return false;
        };
        if !p.offer_due || !p.negotiated || p.pending.is_some() {
            return false;
        }
        if !p
            .out
            .values()
            .any(|t| matches!(t.state, OutState::ToOpen | OutState::ToStop(_)))
        {
            p.offer_due = false;
            return false;
        }
        let users: HashMap<ParticipantId, UserId> =
            self.parts.iter().map(|(k, v)| (*k, v.user)).collect();
        let Some(p) = self.parts.get_mut(&id) else {
            return false;
        };
        p.offer_due = false;
        let mut change = p.rtc.sdp_api();
        for ((publisher, source), t) in p.out.iter_mut() {
            match t.state {
                OutState::ToOpen => {
                    let stream = users.get(publisher).map(|u| source.stream_id(*u));
                    let mid =
                        change.add_media(source.kind(), Direction::SendOnly, stream, None, None);
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
        // The music lines list the stereo payload type only, every other audio line the mono one.
        let mut music_mids: HashSet<String> = p
            .out
            .iter()
            .filter(|((_, s), _)| *s == TrackSource::Music)
            .filter_map(|(_, t)| t.state.mid())
            .map(|m| m.to_string())
            .collect();
        music_mids.extend(p.stereo_mids.iter().cloned());
        p.pending = Some(pending);
        let sdp = restrict_opus_lines(&offer.to_sdp_string(), &music_mids);
        self.signal(id, op::SDP_OFFER, &wire::Sdp { sdp });
        true
    }
}

/// The largest layer a subscriber's hint allows.
fn hint_cap(hint: Option<&SubscriptionHint>, full_height: u32, source: TrackSource) -> Layer {
    match hint {
        Some(h) => match (h.layer, h.viewport) {
            (Some(l), _) => l,
            (None, Some(vp)) => layer_for_viewport(vp, full_height),
            (None, None) => default_cap(source),
        },
        None => default_cap(source),
    }
}

/// Cameras start as thumbnails; a screen is a single layer anyway.
fn default_cap(source: TrackSource) -> Layer {
    if source == TrackSource::Screen {
        Layer::High
    } else {
        Layer::Low
    }
}

#[cfg(test)]
mod video_quality_tests {
    use super::*;
    use crate::governor::MAX_SEVERITY;

    #[test]
    fn full_video_quality_ignores_how_busy_the_room_is() {
        let auto = Flags::default();
        let full = Flags {
            video_full: true,
            ..Flags::default()
        };
        // A free-tier member of a busy room is capped to the smallest layer ...
        assert_eq!(cap_of(MAX_SEVERITY, &auto), Layer::Low);
        // ... unless the channel asked for full quality.
        assert_eq!(cap_of(MAX_SEVERITY, &full), Layer::High);
        assert_eq!(cap_of(0, &auto), Layer::High);
        // The adaptive default still follows the tier.
        let patron = Flags {
            tier: 4,
            ..Flags::default()
        };
        assert_eq!(cap_of(MAX_SEVERITY - 1, &patron), Layer::Mid);
    }
}
