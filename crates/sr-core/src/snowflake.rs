//! Snowflake IDs (FND-CORE-001). Discord layout: 42-bit ms timestamp since the
//! SIDE ROOM epoch, 5-bit worker, 5-bit process, 12-bit increment.

use crate::time::Clock;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;

/// 2026-01-01T00:00:00Z in Unix milliseconds.
pub const SIDEROOM_EPOCH_MS: i64 = 1_767_225_600_000;

const SEQ_BITS: u32 = 12;
const PROCESS_BITS: u32 = 5;
const WORKER_BITS: u32 = 5;
const TS_MASK: u64 = (1 << 42) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Snowflake(pub u64);

impl Snowflake {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
    /// Milliseconds since the SIDE ROOM epoch.
    pub const fn timestamp_since_epoch_ms(self) -> u64 {
        self.0 >> (SEQ_BITS + PROCESS_BITS + WORKER_BITS)
    }
    /// Creation time as Unix milliseconds.
    pub const fn unix_ms(self) -> i64 {
        self.timestamp_since_epoch_ms() as i64 + SIDEROOM_EPOCH_MS
    }
    pub const fn worker(self) -> u8 {
        ((self.0 >> (SEQ_BITS + PROCESS_BITS)) & 0x1f) as u8
    }
    pub const fn process(self) -> u8 {
        ((self.0 >> SEQ_BITS) & 0x1f) as u8
    }
    pub const fn sequence(self) -> u16 {
        (self.0 & 0xfff) as u16
    }
}

impl fmt::Display for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Snowflake {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>().map(Self)
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
    /// Accepts a decimal string (canonical) or an unsigned integer.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = Snowflake;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a snowflake as a string or unsigned integer")
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Snowflake, E> {
                Ok(Snowflake(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Snowflake, E> {
                u64::try_from(v).map(Snowflake).map_err(E::custom)
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Snowflake, E> {
                v.parse().map_err(E::custom)
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SnowflakeError {
    #[error("worker id must be < 32")]
    WorkerOutOfRange,
    #[error("process id must be < 32")]
    ProcessOutOfRange,
}

#[derive(Debug)]
struct State {
    last_ts: u64,
    seq: u64,
}

/// Monotonic, thread-safe generator. Never emits a duplicate or a smaller ID,
/// even if the clock steps backwards or the per-ms sequence overflows (the
/// logical timestamp is advanced instead of sleeping).
pub struct SnowflakeGenerator<C: Clock> {
    clock: C,
    worker: u64,
    process: u64,
    state: Mutex<State>,
}

impl<C: Clock> SnowflakeGenerator<C> {
    pub fn new(clock: C, worker: u8, process: u8) -> Result<Self, SnowflakeError> {
        if worker >= 32 {
            return Err(SnowflakeError::WorkerOutOfRange);
        }
        if process >= 32 {
            return Err(SnowflakeError::ProcessOutOfRange);
        }
        Ok(Self {
            clock,
            worker: u64::from(worker),
            process: u64::from(process),
            state: Mutex::new(State { last_ts: 0, seq: 0 }),
        })
    }

    pub fn generate(&self) -> Snowflake {
        let now = (self.clock.now_unix_ms() - SIDEROOM_EPOCH_MS).max(0) as u64;
        // State is plain integers, so a poisoned lock is still consistent.
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if now > st.last_ts {
            st.last_ts = now;
            st.seq = 0;
        } else {
            st.seq += 1;
            if st.seq >= (1 << SEQ_BITS) {
                st.last_ts += 1;
                st.seq = 0;
            }
        }
        let ts = st.last_ts & TS_MASK;
        Snowflake(
            (ts << (SEQ_BITS + PROCESS_BITS + WORKER_BITS))
                | (self.worker << (SEQ_BITS + PROCESS_BITS))
                | (self.process << SEQ_BITS)
                | st.seq,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::ManualClock;
    use std::collections::HashSet;

    fn gen_at(ms: i64) -> SnowflakeGenerator<ManualClock> {
        SnowflakeGenerator::new(ManualClock::new(ms), 3, 7).unwrap()
    }

    #[test]
    fn layout_roundtrip() {
        let g = gen_at(SIDEROOM_EPOCH_MS + 12_345);
        let id = g.generate();
        assert_eq!(id.timestamp_since_epoch_ms(), 12_345);
        assert_eq!(id.unix_ms(), SIDEROOM_EPOCH_MS + 12_345);
        assert_eq!((id.worker(), id.process(), id.sequence()), (3, 7, 0));
        assert_eq!(g.generate().sequence(), 1);
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(SnowflakeGenerator::new(ManualClock::new(0), 32, 0).is_err());
        assert!(SnowflakeGenerator::new(ManualClock::new(0), 0, 32).is_err());
    }

    #[test]
    fn monotonic_across_overflow_and_clock_regression() {
        let g = gen_at(SIDEROOM_EPOCH_MS + 1000);
        let mut prev = g.generate();
        let mut seen = HashSet::from([prev]);
        for i in 0..10_000 {
            if i == 5000 {
                g.clock.set(SIDEROOM_EPOCH_MS + 10); // clock goes backwards
            }
            let id = g.generate();
            assert!(id > prev);
            assert!(seen.insert(id));
            prev = id;
        }
    }

    #[test]
    fn serde_string() {
        let id = Snowflake(175_928_847_299_117_063);
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            "\"175928847299117063\""
        );
        assert_eq!(
            serde_json::from_str::<Snowflake>("\"175928847299117063\"").unwrap(),
            id
        );
        assert_eq!(
            serde_json::from_str::<Snowflake>("42").unwrap(),
            Snowflake(42)
        );
        assert!(serde_json::from_str::<Snowflake>("\"abc\"").is_err());
        assert!(serde_json::from_str::<Snowflake>("-1").is_err());
    }
}
