//! The per-lobby sub-room (ARC-GST-001, roadmap P28 wave 43, D-385, D-397, D-399): a small relay for the players of
//! one Arcade game session, guests included. Only the data leg is built; the media leg is designed in
//! `docs/dev/design/ingame-voice.md` and reserved by [`MediaScope`].
//!
//! # What a lobby is not
//!
//! A lobby is **not** a voice room. It has no `ParticipantId`, no MLS group, no epoch of the call, no SFrame key, no
//! channel and no guild: [`crate::core::SfuCore`] and this module share no state and no type that names one. A
//! lobby is keyed by the SHA-256 of its lobby token ([`LobbyId`]) and by nothing else, so there is no value a guest
//! could present that would address a call room, and no call member is ever "in" a lobby by being in a call. The
//! relay carries opaque bytes: the players encrypt them under the lobby key (the `#key=` fragment of the link, which
//! the server never sees), so the relay holds no key that could decrypt a frame.
//!
//! # Keys
//!
//! "Its own per-lobby keys": every lobby has a fresh random [`LobbyKeys::key_id`] and an `epoch`, announced to each
//! seat so a client can tell a rekeyed lobby from the previous one. The data and (later) media keys the players use
//! are derived from the link's key with the labels [`DATA_KEY_LABEL`] and [`MEDIA_KEY_LABEL`], which no MLS
//! exporter label of the call ever equals (a test pins that).
//!
//! # Delivery
//!
//! [`LobbyHub::attach`] gives a seat an event stream; [`LobbyHub::send`] relays a frame to every other seat. A seat
//! whose stream is full loses the frame (the game's own state sync recovers, as on the call's data channel); a
//! sender over its budget loses its frame; a frame over [`MAX_PAYLOAD`] is refused. [`LobbyHub::close`] ends a lobby
//! (revoked link, ended Activity) and tells every seat.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::mpsc;

use crate::datachannel::Budget;

/// The largest frame a seat may send: 16 KiB of message and the data frame trailer, as on the call's data channel.
pub const MAX_PAYLOAD: usize = crate::datachannel::MAX_PAYLOAD;
/// Seats one lobby holds (members and guests).
pub const MAX_SEATS: usize = 32;
/// Events one seat may have waiting before frames to it are dropped.
pub const QUEUE: usize = 128;
/// Default sending budget of a seat, kilobits per second.
pub const DEFAULT_KBPS: u32 = 256;
/// The key-derivation label of a lobby's data frames.
pub const DATA_KEY_LABEL: &str = "sideroom/lobby/v1/data";
/// The key-derivation label of a lobby's media (reserved for `ingame_voice`).
pub const MEDIA_KEY_LABEL: &str = "sideroom/lobby/v1/media";

/// A lobby's name inside the relay: the SHA-256 of its lobby token. The raw token is never held here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LobbyId(pub [u8; 32]);

/// One seat of a lobby.
pub type Seat = u32;

/// Who sits in a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A signed-in member of the Activity (the host runs the game).
    Member,
    /// A person who joined through a guest link.
    Guest,
}

/// What the other seats are told about a seat. A member is only ever a number and a role: the lobby carries no
/// account id, handle or avatar (D-397).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub seat: Seat,
    pub role: Role,
    pub callsign: String,
}

/// Which media a lobby carries. Only the data leg exists; `Reserved` marks where the media leg attaches
/// (`docs/dev/design/ingame-voice.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaScope {
    /// Not built: the lobby refuses media of every kind.
    Reserved,
}

/// What a lobby carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    /// Opaque data frames between the seats.
    pub data: bool,
    /// Media between the seats (reserved).
    pub media: MediaScope,
}

/// The identity of a lobby's key material. The keys themselves live with the players.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LobbyKeys {
    pub key_id: [u8; 16],
    pub epoch: u32,
}

impl LobbyKeys {
    /// `key_id` as 32 lowercase hex digits.
    pub fn key_id_hex(&self) -> String {
        self.key_id.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Why a lobby ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The lobby host revoked the link, or it expired.
    Revoked,
    /// The Activity ended.
    Ended,
}

/// What a seat is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Joined(PeerInfo),
    Left(Seat),
    Data { from: Seat, payload: Arc<[u8]> },
    Closed(CloseReason),
}

/// Why the relay refused something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LobbyError {
    #[error("the lobby is full")]
    Full,
    #[error("the frame is too large")]
    TooLarge,
    #[error("the sender is over its budget")]
    OverBudget,
    #[error("no such seat")]
    NoSeat,
}

/// A seat that was just taken.
#[derive(Debug)]
pub struct Attached {
    pub seat: Seat,
    /// Who else is in the lobby now.
    pub peers: Vec<PeerInfo>,
    pub keys: LobbyKeys,
    pub scope: Scope,
    pub events: mpsc::Receiver<Event>,
}

struct SeatState {
    info: PeerInfo,
    tx: mpsc::Sender<Event>,
    budget: Budget,
}

struct Room {
    keys: LobbyKeys,
    next_seat: Seat,
    seats: HashMap<Seat, SeatState>,
}

/// Counters for metrics and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub relayed: u64,
    pub dropped_budget: u64,
    pub dropped_backpressure: u64,
}

struct Inner {
    rooms: HashMap<LobbyId, Room>,
    stats: Stats,
}

/// All lobbies of the process. Cheap to clone; every method is short and never awaits, so it is safe to call from
/// any task.
#[derive(Clone)]
pub struct LobbyHub {
    inner: Arc<Mutex<Inner>>,
    kbps: u32,
    max_seats: usize,
}

impl Default for LobbyHub {
    fn default() -> Self {
        Self::new(DEFAULT_KBPS, MAX_SEATS)
    }
}

fn random_key_id() -> [u8; 16] {
    let mut b = [0u8; 16];
    // A failing system RNG would give every lobby the same id; ids only tell a rekey apart, never protect data.
    if getrandom::fill(&mut b).is_err() {
        b = [0u8; 16];
    }
    b
}

impl LobbyHub {
    pub fn new(kbps: u32, max_seats: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                rooms: HashMap::new(),
                stats: Stats::default(),
            })),
            kbps,
            max_seats,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock only ever leaves counters or a seat map half updated; keep serving.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// What every lobby carries today.
    pub const fn scope() -> Scope {
        Scope {
            data: true,
            media: MediaScope::Reserved,
        }
    }

    /// Take a seat in `lobby`, creating the lobby (with its own fresh keys) if it is not open yet. The other seats
    /// are told. `Full` when the lobby already holds the most seats.
    pub fn attach(
        &self,
        lobby: LobbyId,
        role: Role,
        callsign: &str,
    ) -> Result<Attached, LobbyError> {
        let mut g = self.lock();
        let room = g.rooms.entry(lobby).or_insert_with(|| Room {
            keys: LobbyKeys {
                key_id: random_key_id(),
                epoch: 0,
            },
            next_seat: 1,
            seats: HashMap::new(),
        });
        if room.seats.len() >= self.max_seats {
            return Err(LobbyError::Full);
        }
        let seat = room.next_seat;
        room.next_seat += 1;
        let (tx, events) = mpsc::channel(QUEUE);
        let info = PeerInfo {
            seat,
            role,
            callsign: callsign.chars().take(40).collect(),
        };
        let mut peers: Vec<PeerInfo> = room.seats.values().map(|s| s.info.clone()).collect();
        peers.sort_by_key(|p| p.seat);
        for s in room.seats.values() {
            let _ = s.tx.try_send(Event::Joined(info.clone()));
        }
        room.seats.insert(
            seat,
            SeatState {
                info,
                tx,
                budget: Budget::new(self.kbps),
            },
        );
        Ok(Attached {
            seat,
            peers,
            keys: room.keys,
            scope: Self::scope(),
            events,
        })
    }

    /// Give a seat up. The last one out closes the lobby (its keys are forgotten).
    pub fn detach(&self, lobby: LobbyId, seat: Seat) {
        let mut g = self.lock();
        let Some(room) = g.rooms.get_mut(&lobby) else {
            return;
        };
        if room.seats.remove(&seat).is_none() {
            return;
        }
        for s in room.seats.values() {
            let _ = s.tx.try_send(Event::Left(seat));
        }
        if room.seats.is_empty() {
            g.rooms.remove(&lobby);
        }
    }

    /// Relay `payload` from `seat` to every other seat of `lobby`. Returns how many seats got it.
    pub fn send(
        &self,
        lobby: LobbyId,
        seat: Seat,
        payload: &[u8],
        now: Instant,
    ) -> Result<usize, LobbyError> {
        if payload.len() > MAX_PAYLOAD {
            return Err(LobbyError::TooLarge);
        }
        let mut g = self.lock();
        let Inner { rooms, stats } = &mut *g;
        let room = rooms.get_mut(&lobby).ok_or(LobbyError::NoSeat)?;
        let sender = room.seats.get_mut(&seat).ok_or(LobbyError::NoSeat)?;
        if !sender.budget.allow(now, payload.len()) {
            stats.dropped_budget += 1;
            return Err(LobbyError::OverBudget);
        }
        let shared: Arc<[u8]> = Arc::from(payload);
        let mut delivered = 0;
        for (&to, s) in &room.seats {
            if to == seat {
                continue;
            }
            match s.tx.try_send(Event::Data {
                from: seat,
                payload: Arc::clone(&shared),
            }) {
                Ok(()) => {
                    delivered += 1;
                    stats.relayed += 1;
                }
                Err(_) => stats.dropped_backpressure += 1,
            }
        }
        Ok(delivered)
    }

    /// End a lobby: every seat is told why, then the lobby is forgotten.
    pub fn close(&self, lobby: LobbyId, reason: CloseReason) {
        let mut g = self.lock();
        if let Some(room) = g.rooms.remove(&lobby) {
            for s in room.seats.values() {
                let _ = s.tx.try_send(Event::Closed(reason));
            }
        }
    }

    /// Seats in `lobby` now.
    pub fn seats(&self, lobby: LobbyId) -> usize {
        self.lock().rooms.get(&lobby).map_or(0, |r| r.seats.len())
    }

    /// Open lobbies.
    pub fn lobbies(&self) -> usize {
        self.lock().rooms.len()
    }

    pub fn stats(&self) -> Stats {
        self.lock().stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> LobbyId {
        LobbyId([n; 32])
    }

    #[test]
    fn frames_reach_the_other_seats_only() {
        let hub = LobbyHub::default();
        let mut a = hub.attach(id(1), Role::Member, "Host").unwrap();
        let mut b = hub.attach(id(1), Role::Guest, "Amber Otter 42").unwrap();
        let mut c = hub.attach(id(2), Role::Guest, "Other lobby").unwrap();
        assert_eq!(b.peers.len(), 1);
        assert!(
            matches!(a.events.try_recv(), Ok(Event::Joined(p)) if p.callsign == "Amber Otter 42")
        );
        assert_eq!(hub.send(id(1), b.seat, b"hello", Instant::now()), Ok(1));
        match a.events.try_recv() {
            Ok(Event::Data { from, payload }) => {
                assert_eq!(from, b.seat);
                assert_eq!(&*payload, b"hello");
            }
            other => panic!("{other:?}"),
        }
        assert!(b.events.try_recv().is_err(), "the sender hears no echo");
        assert!(c.events.try_recv().is_err(), "another lobby hears nothing");
        // A lobby nobody sits in has no seats to send from.
        assert_eq!(
            hub.send(id(3), c.seat, b"x", Instant::now()),
            Err(LobbyError::NoSeat)
        );
    }

    #[test]
    fn a_lobby_has_its_own_keys_and_forgets_them() {
        let hub = LobbyHub::default();
        let a = hub.attach(id(1), Role::Member, "Host").unwrap();
        let b = hub.attach(id(2), Role::Member, "Host").unwrap();
        let again = hub.attach(id(1), Role::Guest, "G").unwrap();
        assert_ne!(a.keys.key_id, b.keys.key_id);
        assert_eq!(a.keys, again.keys);
        assert_eq!(a.keys.key_id_hex().len(), 32);
        hub.detach(id(1), a.seat);
        hub.detach(id(1), again.seat);
        assert_eq!(hub.lobbies(), 1);
        let fresh = hub.attach(id(1), Role::Member, "Host").unwrap();
        assert_ne!(
            fresh.keys.key_id, a.keys.key_id,
            "a reopened lobby is rekeyed"
        );
    }

    #[test]
    fn the_lobby_carries_data_and_no_media_and_no_call_labels() {
        let s = LobbyHub::scope();
        assert!(s.data);
        assert_eq!(s.media, MediaScope::Reserved);
        for l in [DATA_KEY_LABEL, MEDIA_KEY_LABEL] {
            assert!(l.starts_with("sideroom/lobby/"));
            assert!(!l.contains("mls") && !l.contains("sframe") && !l.contains("voice"));
        }
        assert_ne!(DATA_KEY_LABEL, MEDIA_KEY_LABEL);
    }

    #[test]
    fn size_budget_and_seat_limits_hold() {
        let hub = LobbyHub::new(8, 2);
        let a = hub.attach(id(1), Role::Member, "Host").unwrap();
        let _b = hub.attach(id(1), Role::Guest, "G").unwrap();
        assert_eq!(
            hub.attach(id(1), Role::Guest, "H").unwrap_err(),
            LobbyError::Full
        );
        let big = vec![0u8; MAX_PAYLOAD + 1];
        assert_eq!(
            hub.send(id(1), a.seat, &big, Instant::now()),
            Err(LobbyError::TooLarge)
        );
        let now = Instant::now();
        let full = vec![0u8; MAX_PAYLOAD];
        assert_eq!(hub.send(id(1), a.seat, &full, now), Ok(1));
        assert_eq!(
            hub.send(id(1), a.seat, &full, now),
            Err(LobbyError::OverBudget)
        );
        assert_eq!(hub.stats().dropped_budget, 1);
    }

    #[test]
    fn a_slow_seat_loses_frames_without_blocking_the_rest() {
        let hub = LobbyHub::new(100_000, 4);
        let a = hub.attach(id(1), Role::Member, "Host").unwrap();
        let _slow = hub.attach(id(1), Role::Guest, "Slow").unwrap();
        let now = Instant::now();
        for _ in 0..QUEUE + 20 {
            let _ = hub.send(id(1), a.seat, b"x", now);
        }
        assert!(hub.stats().dropped_backpressure > 0);
    }

    #[test]
    fn closing_tells_every_seat_and_forgets_the_lobby() {
        let hub = LobbyHub::default();
        let mut a = hub.attach(id(1), Role::Member, "Host").unwrap();
        let mut b = hub.attach(id(1), Role::Guest, "G").unwrap();
        hub.close(id(1), CloseReason::Revoked);
        assert_eq!(
            a.events.try_recv(),
            Ok(Event::Joined(PeerInfo {
                seat: b.seat,
                role: Role::Guest,
                callsign: "G".into()
            }))
        );
        assert_eq!(a.events.try_recv(), Ok(Event::Closed(CloseReason::Revoked)));
        assert_eq!(b.events.try_recv(), Ok(Event::Closed(CloseReason::Revoked)));
        assert_eq!(hub.lobbies(), 0);
        assert_eq!(
            hub.send(id(1), a.seat, b"x", Instant::now()),
            Err(LobbyError::NoSeat)
        );
    }

    #[test]
    fn leaving_is_announced_and_the_last_one_out_closes_the_lobby() {
        let hub = LobbyHub::default();
        let mut a = hub.attach(id(1), Role::Member, "Host").unwrap();
        let b = hub.attach(id(1), Role::Guest, "G").unwrap();
        let _ = a.events.try_recv();
        hub.detach(id(1), b.seat);
        assert_eq!(a.events.try_recv(), Ok(Event::Left(b.seat)));
        hub.detach(id(1), b.seat);
        hub.detach(id(1), a.seat);
        assert_eq!(hub.lobbies(), 0);
    }
}
