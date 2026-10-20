//! The media-path block of a takedown (spec 5, step 3: "block the stored object on the media/CDN path").
//!
//! `blocked_objects` lists what an upheld takedown took down: single objects, key prefixes and Cinema assets.
//! The CDN route asks [`MediaBlocklist`] before it serves; the answer comes from a copy of the table that is
//! reloaded when it is older than the time-to-live, so a request never waits for the database once the copy is
//! warm. The block therefore reaches every node within the time-to-live (30 seconds by default), and the node
//! that took the content down reloads at once ([`MediaBlocklist::refresh`]).

use std::collections::HashSet;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use sqlx::{PgPool, Row};

/// How long a loaded copy is trusted.
pub const DEFAULT_TTL: Duration = Duration::from_secs(30);

/// A loaded copy of the table.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BlockSet {
    keys: HashSet<String>,
    prefixes: Vec<String>,
    assets: HashSet<i64>,
}

impl BlockSet {
    /// Build from `(kind, value)` rows; rows of an unknown kind or an unparsable asset id are ignored.
    pub fn from_rows(rows: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut s = Self::default();
        for (kind, value) in rows {
            match kind.as_str() {
                "key" => {
                    s.keys.insert(value);
                }
                "prefix" => s.prefixes.push(value),
                "hls_asset" => {
                    if let Ok(id) = value.parse() {
                        s.assets.insert(id);
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// Whether the object `key` is blocked: named exactly, or under a blocked prefix.
    pub fn blocks_key(&self, key: &str) -> bool {
        self.keys.contains(key) || self.prefixes.iter().any(|p| key.starts_with(p.as_str()))
    }

    /// Whether the Cinema asset is blocked.
    pub fn blocks_asset(&self, asset: i64) -> bool {
        self.assets.contains(&asset)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.prefixes.is_empty() && self.assets.is_empty()
    }
}

struct Cached {
    loaded: Option<Instant>,
    set: BlockSet,
}

/// The shared, cached view of `blocked_objects`.
pub struct MediaBlocklist {
    pool: Option<PgPool>,
    ttl: Duration,
    cached: RwLock<Cached>,
}

impl MediaBlocklist {
    pub fn new(pool: PgPool) -> Self {
        Self::with_ttl(pool, DEFAULT_TTL)
    }

    pub fn with_ttl(pool: PgPool, ttl: Duration) -> Self {
        Self {
            pool: Some(pool),
            ttl,
            cached: RwLock::new(Cached {
                loaded: None,
                set: BlockSet::default(),
            }),
        }
    }

    /// A list that never reloads (tests of the routes that consult it).
    pub fn fixed(set: BlockSet) -> Self {
        Self {
            pool: None,
            ttl: Duration::MAX,
            cached: RwLock::new(Cached {
                loaded: Some(Instant::now()),
                set,
            }),
        }
    }

    /// Reload from the database now. A failed load keeps the copy that was there and logs.
    pub async fn refresh(&self) {
        let Some(pool) = &self.pool else { return };
        let rows = sqlx::query("SELECT kind, value FROM blocked_objects")
            .fetch_all(pool)
            .await;
        match rows {
            Ok(rows) => {
                let set = BlockSet::from_rows(
                    rows.iter()
                        .filter_map(|r| Some((r.try_get("kind").ok()?, r.try_get("value").ok()?))),
                );
                if let Ok(mut c) = self.cached.write() {
                    c.set = set;
                    c.loaded = Some(Instant::now());
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "reloading the media blocklist failed; keeping the last copy");
                if let Ok(mut c) = self.cached.write() {
                    // Try again after a short pause rather than on every request.
                    c.loaded = Some(Instant::now());
                }
            }
        }
    }

    async fn fresh(&self) {
        let stale = self
            .cached
            .read()
            .map(|c| c.loaded.is_none_or(|t| t.elapsed() >= self.ttl))
            .unwrap_or(true);
        if stale {
            self.refresh().await;
        }
    }

    /// Whether the stored object `key` must not be served.
    pub async fn is_blocked_key(&self, key: &str) -> bool {
        self.fresh().await;
        self.cached.read().is_ok_and(|c| c.set.blocks_key(key))
    }

    /// Whether the Cinema asset must not be served.
    pub async fn is_blocked_asset(&self, asset: i64) -> bool {
        self.fresh().await;
        self.cached.read().is_ok_and(|c| c.set.blocks_asset(asset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> BlockSet {
        BlockSet::from_rows([
            ("key".to_owned(), "chat/1/2/a.png".to_owned()),
            ("prefix".to_owned(), "library/7/".to_owned()),
            ("hls_asset".to_owned(), "42".to_owned()),
            ("hls_asset".to_owned(), "not-a-number".to_owned()),
            ("other".to_owned(), "x".to_owned()),
        ])
    }

    #[test]
    fn exact_keys_and_prefixes_block() {
        let s = set();
        assert!(s.blocks_key("chat/1/2/a.png"));
        assert!(!s.blocks_key("chat/1/2/a.png.bak"));
        assert!(s.blocks_key("library/7/9/001.webp"));
        // A prefix names a directory: library/70/ is another series.
        assert!(!s.blocks_key("library/70/9/001.webp"));
        assert!(!s.blocks_key("library/8/9/001.webp"));
    }

    #[test]
    fn assets_block_by_id() {
        let s = set();
        assert!(s.blocks_asset(42));
        assert!(!s.blocks_asset(43));
        assert!(!BlockSet::default().blocks_asset(42));
        assert!(BlockSet::default().is_empty());
        assert!(!s.is_empty());
    }
}
