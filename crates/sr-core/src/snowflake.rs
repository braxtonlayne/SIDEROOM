//! Snowflake IDs (docs/02-architecture.md §7).
//!
//! Discord layout: 42-bit milliseconds since the SIDE ROOM epoch
//! (2026-01-01T00:00:00Z), 5-bit worker, 5-bit process, 12-bit increment.
//! Serialized as strings in JSON, accepted as strings or integers.

use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::clock::Clock;

/// 2026-01-01T00:00:00Z in Unix milliseconds.
pub const SIDE_ROOM_EPOCH_MS: i64 = 1_767_225_600_000;

const WORKER_BITS: u64 = 5;
const PROCESS_BITS: u64 = 5;
const INCREMENT_BITS: u64 = 12;
const MAX_WORKER: u8 = (1 << WORKER_BITS) - 1;
const MAX_PROCESS: u8 = (1 << PROCESS_BITS) - 1;
const MAX_INCREMENT: u16 = (1 << INCREMENT_BITS) - 1;
const TIMESTAMP_SHIFT: u64 = WORKER_BITS + PROCESS_BITS + INCREMENT_BITS;

/// A 64-bit Snowflake identifier.
///
/// Stored in Postgres as `BIGINT` (the top bit is never set before year
/// 2165, so the `i64` conversion is lossless in practice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Snowflake(pub u64);

impl Snowflake {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// Value for a Postgres `BIGINT` column.
    pub const fn as_i64(self) -> i64 {
        self.0 as i64
    }

    pub const fn from_i64(v: i64) -> Self {
        Self(v as u64)
    }

    /// Milliseconds since the Unix epoch encoded in this ID.
    pub const fn timestamp_ms(self) -> i64 {
        (self.0 >> TIMESTAMP_SHIFT) as i64 + SIDE_ROOM_EPOCH_MS
    }

    pub fn created_at(self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.timestamp_ms())
            .single()
            .unwrap_or(DateTime::<Utc>::MIN_UTC)
    }

    pub const fn worker(self) -> u8 {
        ((self.0 >> (PROCESS_BITS + INCREMENT_BITS)) & MAX_WORKER as u64) as u8
    }

    pub const fn process(self) -> u8 {
        ((self.0 >> INCREMENT_BITS) & MAX_PROCESS as u64) as u8
    }

    pub const fn increment(self) -> u16 {
        (self.0 & MAX_INCREMENT as u64) as u16
    }

    /// Smallest Snowflake that could have been created at `at`. Used for
    /// `before`/`after` pagination by time and for partition bounds.
    pub fn from_timestamp(at: DateTime<Utc>) -> Self {
        let ms = (at.timestamp_millis() - SIDE_ROOM_EPOCH_MS).max(0) as u64;
        Self(ms << TIMESTAMP_SHIFT)
    }
}

impl fmt::Display for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid snowflake")]
pub struct ParseSnowflakeError;

impl FromStr for Snowflake {
    type Err = ParseSnowflakeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>().map(Self).map_err(|_| ParseSnowflakeError)
    }
}

impl From<u64> for Snowflake {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl Serialize for Snowflake {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Snowflake {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Snowflake;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a snowflake as a string or integer")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Snowflake, E> {
                v.parse().map_err(|_| E::custom("invalid snowflake"))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Snowflake, E> {
                Ok(Snowflake(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Snowflake, E> {
                u64::try_from(v)
                    .map(Snowflake)
                    .map_err(|_| E::custom("negative snowflake"))
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GeneratorError {
    #[error("worker id {0} exceeds {MAX_WORKER}")]
    Worker(u8),
    #[error("process id {0} exceeds {MAX_PROCESS}")]
    Process(u8),
}

struct State {
    last_ms: u64,
    increment: u16,
}

/// Thread-safe, monotonic Snowflake generator.
///
/// If the clock goes backwards, IDs keep using the last seen timestamp so
/// they never decrease. If 4096 IDs are issued within one millisecond the
/// generator borrows from the next millisecond.
pub struct SnowflakeGenerator<C: Clock> {
    clock: C,
    prefix: u64,
    state: Mutex<State>,
}

impl<C: Clock> SnowflakeGenerator<C> {
    pub fn new(clock: C, worker: u8, process: u8) -> Result<Self, GeneratorError> {
        if worker > MAX_WORKER {
            return Err(GeneratorError::Worker(worker));
        }
        if process > MAX_PROCESS {
            return Err(GeneratorError::Process(process));
        }
        Ok(Self {
            clock,
            prefix: (u64::from(worker) << (PROCESS_BITS + INCREMENT_BITS))
                | (u64::from(process) << INCREMENT_BITS),
            state: Mutex::new(State {
                last_ms: 0,
                increment: 0,
            }),
        })
    }

    pub fn next_id(&self) -> Snowflake {
        let now = (self.clock.now().timestamp_millis() - SIDE_ROOM_EPOCH_MS).max(0) as u64;
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if now > st.last_ms {
            st.last_ms = now;
            st.increment = 0;
        } else if st.increment == MAX_INCREMENT {
            st.last_ms += 1;
            st.increment = 0;
        } else {
            st.increment += 1;
        }
        Snowflake((st.last_ms << TIMESTAMP_SHIFT) | self.prefix | u64::from(st.increment))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;

    #[test]
    fn epoch_constant_matches_date() {
        let epoch = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(epoch.timestamp_millis(), SIDE_ROOM_EPOCH_MS);
    }

    #[test]
    fn layout_roundtrip() {
        let at = Utc.with_ymd_and_hms(2026, 10, 11, 12, 0, 0).unwrap();
        let g = SnowflakeGenerator::new(FixedClock::new(at), 3, 7).unwrap();
        let a = g.next_id();
        let b = g.next_id();
        assert_eq!(a.created_at(), at);
        assert_eq!((a.worker(), a.process(), a.increment()), (3, 7, 0));
        assert_eq!(b.increment(), 1);
        assert!(b > a);
    }

    #[test]
    fn monotonic_when_increment_overflows_or_clock_regresses() {
        let at = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
        let clock = FixedClock::new(at);
        let g = SnowflakeGenerator::new(clock.clone(), 0, 0).unwrap();
        let mut prev = g.next_id();
        for _ in 0..10_000 {
            let id = g.next_id();
            assert!(id > prev);
            prev = id;
        }
        clock.set(at - chrono::Duration::seconds(5));
        assert!(g.next_id() > prev);
    }

    #[test]
    fn serde_string_and_int() {
        let id = Snowflake(175_928_847_299_117_063);
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            "\"175928847299117063\""
        );
        let a: Snowflake = serde_json::from_str("\"175928847299117063\"").unwrap();
        let b: Snowflake = serde_json::from_str("175928847299117063").unwrap();
        assert_eq!(a, id);
        assert_eq!(b, id);
        assert!(serde_json::from_str::<Snowflake>("\"x\"").is_err());
    }

    #[test]
    fn rejects_out_of_range_bits() {
        assert!(SnowflakeGenerator::new(SystemClockForTest, 32, 0).is_err());
        assert!(SnowflakeGenerator::new(SystemClockForTest, 0, 32).is_err());
    }

    struct SystemClockForTest;
    impl Clock for SystemClockForTest {
        fn now(&self) -> DateTime<Utc> {
            Utc::now()
        }
    }
}
