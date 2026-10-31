//! Activity data channels through the SFU (ACT-CH-001): the framing, the per-member budget and the
//! instance registry. Sans-IO like the rest of the core; [`crate::core::SfuCore`] drives it from str0m's
//! channel events.
//!
//! Every participant opens one SCTP data channel labelled [`LABEL`] when it joins the call (the browser's
//! `createDataChannel`, ordered and reliable). Everything on it is binary and framed:
//!
//! ```text
//! client -> SFU   kind(1)  instance(8, big-endian)  payload...
//! SFU -> client   kind(1)  instance(8)  sender user id(8)  payload...
//! ```
//!
//! * `kind` [`JOIN`] and [`LEAVE`] carry no payload: the participant starts or stops listening to an
//!   activity instance. An instance is named by the client (the gateway's activity-instance snowflake)
//!   and is scoped to the voice room, so the pair (room, instance id) is the relay scope: a message is
//!   handed only to the other participants of the same room that joined the same instance. Two rooms using
//!   the same instance number never see each other.
//! * `kind` [`DATA`] carries one opaque message. The payload is an `sr-e2ee` data frame (AES-GCM under the
//!   sender's key for the room's epoch); the SFU never parses it, and only the sender may send into an
//!   instance it joined. The SFU adds the sender's user id (authenticated by the connection, not claimed
//!   by the client) so receivers pick the right key.
//!
//! Each member has a byte budget (`voice.datachannel_kbps`, default [`DEFAULT_KBPS`]) enforced as a token
//! bucket on what it sends; a message that does not fit is dropped (the data channel is a side channel; the
//! activity's own state sync recovers, and voice is never slowed by it).

use std::collections::HashSet;
use std::time::Instant;

use sr_core::UserId;

/// The data channel's label.
pub const LABEL: &str = "sr-act";
/// `voice.datachannel_kbps` default.
pub const DEFAULT_KBPS: u32 = 256;

pub const JOIN: u8 = 0;
pub const LEAVE: u8 = 1;
pub const DATA: u8 = 2;

/// Header of a client frame: kind and instance.
pub const HEADER_IN: usize = 9;
/// Header of a relayed frame: kind, instance and sender.
pub const HEADER_OUT: usize = 17;
/// Largest payload relayed: 16 KiB of message and the data frame trailer (`sr_e2ee::data::MAX_WIRE`).
pub const MAX_PAYLOAD: usize = 16 * 1024 + 64;
/// Instances one member may listen to at once.
pub const MAX_INSTANCES: usize = 8;

/// A parsed client frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame<'a> {
    Join { instance: u64 },
    Leave { instance: u64 },
    Data { instance: u64, payload: &'a [u8] },
}

/// Parse a client frame; `None` for anything malformed (dropped).
pub fn parse(buf: &[u8]) -> Option<Frame<'_>> {
    if buf.len() < HEADER_IN || buf.len() - HEADER_IN > MAX_PAYLOAD {
        return None;
    }
    let instance = u64::from_be_bytes(buf[1..9].try_into().ok()?);
    let payload = &buf[HEADER_IN..];
    match buf[0] {
        JOIN if payload.is_empty() => Some(Frame::Join { instance }),
        LEAVE if payload.is_empty() => Some(Frame::Leave { instance }),
        DATA => Some(Frame::Data { instance, payload }),
        _ => None,
    }
}

/// The frame handed to the other members.
pub fn relayed(instance: u64, sender: UserId, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(HEADER_OUT + payload.len());
    v.push(DATA);
    v.extend_from_slice(&instance.to_be_bytes());
    v.extend_from_slice(&sender.get().to_be_bytes());
    v.extend_from_slice(payload);
    v
}

/// A token bucket in bytes: refills at the configured rate, holds one second's worth (at least one
/// largest message, so a full-size message is always possible when the bucket is full).
#[derive(Debug, Clone)]
pub struct Budget {
    rate_bps: f64,
    capacity: f64,
    tokens: f64,
    last: Option<Instant>,
}

impl Budget {
    /// `kbps` kilobits per second; `0` means no data channel at all.
    pub fn new(kbps: u32) -> Self {
        let rate_bps = f64::from(kbps) * 1000.0 / 8.0;
        let capacity = if kbps == 0 {
            0.0
        } else {
            rate_bps.max((MAX_PAYLOAD + HEADER_IN) as f64)
        };
        Budget {
            rate_bps,
            capacity,
            tokens: capacity,
            last: None,
        }
    }

    /// Take `bytes` from the bucket; false (and nothing taken) when it does not fit.
    pub fn allow(&mut self, now: Instant, bytes: usize) -> bool {
        if let Some(last) = self.last {
            let dt = now.saturating_duration_since(last).as_secs_f64();
            self.tokens = (self.tokens + dt * self.rate_bps).min(self.capacity);
        }
        self.last = Some(now);
        let n = bytes as f64;
        if n > self.tokens {
            return false;
        }
        self.tokens -= n;
        true
    }
}

/// One member's data-channel state.
#[derive(Debug, Clone)]
pub struct Member {
    /// The open channel's id, once str0m reports it.
    pub channel: Option<str0m::channel::ChannelId>,
    pub instances: HashSet<u64>,
    pub budget: Budget,
}

impl Member {
    pub fn new(kbps: u32) -> Self {
        Member {
            channel: None,
            instances: HashSet::new(),
            budget: Budget::new(kbps),
        }
    }

    /// Apply a join; false when the member already listens to [`MAX_INSTANCES`] others.
    pub fn join(&mut self, instance: u64) -> bool {
        if self.instances.contains(&instance) {
            return true;
        }
        if self.instances.len() >= MAX_INSTANCES {
            return false;
        }
        self.instances.insert(instance);
        true
    }
}

/// Counters for metrics and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Messages handed to a receiver.
    pub relayed: u64,
    /// Bytes handed to receivers (headers included).
    pub relayed_bytes: u64,
    /// Messages dropped because their sender was over `voice.datachannel_kbps`.
    pub dropped_budget: u64,
    /// Messages dropped for scope: a sender that had not joined the instance, bad frames, text frames,
    /// an unkeyed sender.
    pub dropped_scope: u64,
    /// Messages a receiver's channel could not take (its buffer was full).
    pub dropped_backpressure: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn frame(kind: u8, instance: u64, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![kind];
        v.extend_from_slice(&instance.to_be_bytes());
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn parses_the_three_kinds_and_refuses_the_rest() {
        assert_eq!(
            parse(&frame(JOIN, 5, b"")),
            Some(Frame::Join { instance: 5 })
        );
        assert_eq!(
            parse(&frame(LEAVE, 5, b"")),
            Some(Frame::Leave { instance: 5 })
        );
        assert_eq!(
            parse(&frame(DATA, 5, b"xyz")),
            Some(Frame::Data {
                instance: 5,
                payload: b"xyz"
            })
        );
        assert_eq!(parse(&frame(JOIN, 5, b"junk")), None);
        assert_eq!(parse(&frame(9, 5, b"")), None);
        assert_eq!(parse(&[DATA, 1, 2]), None);
        assert_eq!(parse(&frame(DATA, 1, &vec![0; MAX_PAYLOAD + 1])), None);
        assert!(parse(&frame(DATA, 1, &vec![0; MAX_PAYLOAD])).is_some());
    }

    #[test]
    fn relayed_frames_name_the_sender() {
        let f = relayed(7, UserId::new(42), b"abc");
        assert_eq!(f.len(), HEADER_OUT + 3);
        assert_eq!(f[0], DATA);
        assert_eq!(u64::from_be_bytes(f[1..9].try_into().unwrap()), 7);
        assert_eq!(u64::from_be_bytes(f[9..17].try_into().unwrap()), 42);
        assert_eq!(&f[17..], b"abc");
    }

    #[test]
    fn budget_refills_at_the_configured_rate() {
        let t0 = Instant::now();
        let mut b = Budget::new(256); // 32 000 bytes per second, bucket of one full message (16.4 KB) or a second
        assert!(b.allow(t0, 20_000));
        assert!(b.allow(t0, 10_000));
        assert!(!b.allow(t0, 10_000), "over the bucket");
        // A second later 32 000 bytes are back (capped at the bucket).
        assert!(b.allow(t0 + Duration::from_secs(1), 30_000));
        assert!(!b.allow(t0 + Duration::from_secs(1), 5_000));
        // Steady state: 256 kbps allows 3200 bytes per 100 ms and no more.
        let mut b = Budget::new(256);
        let mut sent = 0usize;
        let mut now = t0;
        assert!(b.allow(now, 32_000)); // drain the burst
        for _ in 0..100 {
            now += Duration::from_millis(100);
            if b.allow(now, 3_000) {
                sent += 3_000;
            }
            if b.allow(now, 3_000) {
                sent += 3_000;
            }
        }
        // Ten seconds at 32 000 bytes per second, give or take one burst.
        assert!((300_000..=340_000).contains(&sent), "{sent}");
    }

    #[test]
    fn a_zero_budget_blocks_everything() {
        let mut b = Budget::new(0);
        assert!(!b.allow(Instant::now(), 1));
    }

    #[test]
    fn a_member_listens_to_a_bounded_number_of_instances() {
        let mut m = Member::new(256);
        for i in 0..MAX_INSTANCES as u64 {
            assert!(m.join(i));
        }
        assert!(m.join(0), "joining again is fine");
        assert!(!m.join(99));
    }
}
