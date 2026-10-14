//! Entitlement arithmetic (pure) and the cached read interface other crates use.
//!
//! Grants are the source of truth (`entitlement_grants`): a credited invoice adds one, a revert or
//! a staff revocation removes it. Everything else (Patron validity, a server's pool and level) is
//! derived from them by the functions in this file, so a reorg is just "recompute without that grant".

use crate::{
    config::{DonationConfig, Limits, effective_limits},
    error::PayResult,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

pub const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantKind {
    /// Patron time for a user.
    Patron,
    /// A contribution to a guild's pool.
    ServerPool,
}

impl GrantKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Patron => "patron",
            Self::ServerPool => "server_pool",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "patron" => Some(Self::Patron),
            "server_pool" => Some(Self::ServerPool),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantSource {
    /// A credited invoice (`source_id` is the invoice).
    Invoice,
    /// Staff (`source_id` is the staff account).
    Admin,
    /// Copied from `cinema_download_grants` by migration 0542.
    Migrated,
}

impl GrantSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::Admin => "admin",
            Self::Migrated => "migrated",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "invoice" => Some(Self::Invoice),
            "admin" => Some(Self::Admin),
            "migrated" => Some(Self::Migrated),
            _ => None,
        }
    }
}

/// One entitlement grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: i64,
    pub kind: GrantKind,
    pub user_id: Option<i64>,
    pub guild_id: Option<i64>,
    pub source: GrantSource,
    pub source_id: Option<i64>,
    /// Patron: days added after the current end (stacking).
    pub days: Option<i64>,
    /// Patron: a fixed end instead of `days` (migrated legacy grants, staff windows).
    pub ends_ms: Option<i64>,
    /// Patron: never ends (staff only).
    pub permanent: bool,
    /// Server pool: USD cents contributed.
    pub usd_cents: Option<i64>,
    /// When the grant took effect (credit time).
    pub starts_ms: i64,
    pub revoked_ms: Option<i64>,
    pub reason: Option<String>,
}

/// Result of stacking all Patron grants of one user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PatronSpan {
    pub permanent: bool,
    /// End of the paid time (`None` when there is none at all, or `permanent`).
    pub until_ms: Option<i64>,
    pub started_ms: Option<i64>,
}

/// Stacks non-revoked Patron grants in credit order: each `days` grant starts when the previous
/// paid time ends (or at its own start if that is later); a fixed-end grant extends the end to
/// at least its own.
pub fn stack_patron(grants: &[Grant]) -> PatronSpan {
    let mut live: Vec<&Grant> = grants
        .iter()
        .filter(|g| g.kind == GrantKind::Patron && g.revoked_ms.is_none())
        .collect();
    live.sort_by_key(|g| (g.starts_ms, g.id));
    let mut span = PatronSpan::default();
    let mut end: Option<i64> = None;
    for g in live {
        if g.permanent {
            span.permanent = true;
            span.started_ms.get_or_insert(g.starts_ms);
            continue;
        }
        let new_end = if let Some(days) = g.days {
            let start = end.map_or(g.starts_ms, |e| e.max(g.starts_ms));
            Some(start + days.max(0) * DAY_MS)
        } else {
            g.ends_ms
                .filter(|e| *e > g.starts_ms)
                .map(|e| end.map_or(e, |c| c.max(e)))
        };
        if let Some(e) = new_end {
            span.started_ms.get_or_insert(g.starts_ms);
            end = Some(e);
        }
    }
    span.until_ms = if span.permanent { None } else { end };
    span
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PatronStatus {
    pub active: bool,
    /// Past the paid time but inside the grace period (perks still on).
    pub in_grace: bool,
    pub permanent: bool,
    pub until_ms: Option<i64>,
    pub started_ms: Option<i64>,
}

impl PatronStatus {
    /// Whether Patron perks apply right now (paid time or grace).
    pub fn perks_on(&self) -> bool {
        self.active || self.in_grace
    }

    pub fn none() -> Self {
        Self {
            active: false,
            in_grace: false,
            permanent: false,
            until_ms: None,
            started_ms: None,
        }
    }
}

pub fn patron_status(span: PatronSpan, now_ms: i64, grace_days: u32) -> PatronStatus {
    if span.permanent {
        return PatronStatus {
            active: true,
            in_grace: false,
            permanent: true,
            until_ms: None,
            started_ms: span.started_ms,
        };
    }
    match span.until_ms {
        Some(u) if now_ms < u => PatronStatus {
            active: true,
            in_grace: false,
            permanent: false,
            until_ms: Some(u),
            started_ms: span.started_ms,
        },
        Some(u) => PatronStatus {
            active: false,
            in_grace: now_ms < u + i64::from(grace_days) * DAY_MS,
            permanent: false,
            until_ms: Some(u),
            started_ms: span.started_ms,
        },
        None => PatronStatus::none(),
    }
}

/// How far along the "your perks end" notices a member is: 0 nothing to say, 1 expiring within
/// three days, 2 in grace, 3 lapsed. The housekeeping job emits an event when this rises.
pub fn notice_level(span: PatronSpan, now_ms: i64, grace_days: u32) -> u8 {
    if span.permanent {
        return 0;
    }
    let Some(until) = span.until_ms else { return 0 };
    let st = patron_status(span, now_ms, grace_days);
    if st.active {
        u8::from(until - now_ms <= 3 * DAY_MS)
    } else if st.in_grace {
        2
    } else {
        3
    }
}

/// Sum of the live pool contributions inside the rolling window.
pub fn pool_cents(grants: &[Grant], now_ms: i64, window_days: u32) -> i64 {
    let since = now_ms - i64::from(window_days) * DAY_MS;
    grants
        .iter()
        .filter(|g| {
            g.kind == GrantKind::ServerPool && g.revoked_ms.is_none() && g.starts_ms > since
        })
        .filter_map(|g| g.usd_cents)
        .sum()
}

/// The highest level whose threshold the pool meets (0..=3).
pub fn level_for_pool(pool: i64, thresholds: [u32; 3]) -> u8 {
    thresholds
        .iter()
        .rposition(|t| pool >= i64::from(*t))
        .map_or(0, |i| (i + 1) as u8)
}

/// Level after a refresh. A drop is held for the grace period (`below_since` marks its start).
pub fn next_level(
    current: u8,
    below_since_ms: Option<i64>,
    pool: i64,
    thresholds: [u32; 3],
    grace_days: u32,
    now_ms: i64,
) -> (u8, Option<i64>) {
    let computed = level_for_pool(pool, thresholds);
    if computed >= current {
        return (computed, None);
    }
    match below_since_ms {
        None => (current, Some(now_ms)),
        Some(t) if now_ms - t >= i64::from(grace_days) * DAY_MS => (computed, None),
        Some(t) => (current, Some(t)),
    }
}

/// What a member's entitlement looks like to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEntitlement {
    pub user_id: i64,
    pub patron: PatronStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildEntitlement {
    pub guild_id: i64,
    pub level: u8,
    pub pool_cents: i64,
    /// While set, the guild is below its level and keeps it until this time.
    pub grace_until_ms: Option<i64>,
}

/// The cached lookup other crates use (voice, media, chat uploads, downloads).
#[async_trait]
pub trait Entitlements: Send + Sync {
    async fn user(&self, user_id: i64) -> PayResult<UserEntitlement>;
    async fn guild(&self, guild_id: i64) -> PayResult<GuildEntitlement>;
    /// `effective_limits(user, context)` (doc 08 section 5): `guild_id` is the server, `None` for DMs.
    async fn limits(&self, user_id: i64, guild_id: Option<i64>) -> PayResult<Limits>;
}

/// Where the entitlement service reads from (implemented by the payment repository).
#[async_trait]
pub trait EntitlementSource: Send + Sync {
    async fn user_entitlement(
        &self,
        user_id: i64,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<PatronStatus>;
    async fn guild_entitlement(&self, guild_id: i64, now_ms: i64) -> PayResult<GuildEntitlement>;
    async fn config(&self) -> PayResult<DonationConfig>;
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The cached [`Entitlements`] implementation. Entries live `ttl` at most and are dropped by
/// [`invalidate_user`](Self::invalidate_user) / [`invalidate_guild`](Self::invalidate_guild),
/// which the payment service calls whenever it emits `SR_ENTITLEMENT_UPDATE`.
pub struct EntitlementService {
    src: Arc<dyn EntitlementSource>,
    clock: Clock,
    users: moka::future::Cache<i64, UserEntitlement>,
    guilds: moka::future::Cache<i64, GuildEntitlement>,
    config: moka::future::Cache<(), DonationConfig>,
}

impl EntitlementService {
    pub fn new(src: Arc<dyn EntitlementSource>, clock: Clock, ttl: Duration) -> Self {
        Self {
            src,
            clock,
            users: moka::future::Cache::builder()
                .max_capacity(50_000)
                .time_to_live(ttl)
                .build(),
            guilds: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(ttl)
                .build(),
            config: moka::future::Cache::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(30))
                .build(),
        }
    }

    pub async fn invalidate_user(&self, user_id: i64) {
        self.users.invalidate(&user_id).await;
    }

    pub async fn invalidate_guild(&self, guild_id: i64) {
        self.guilds.invalidate(&guild_id).await;
    }

    pub async fn invalidate_config(&self) {
        self.config.invalidate(&()).await;
    }

    pub async fn config(&self) -> PayResult<DonationConfig> {
        if let Some(c) = self.config.get(&()).await {
            return Ok(c);
        }
        let c = self.src.config().await?;
        self.config.insert((), c.clone()).await;
        Ok(c)
    }
}

#[async_trait]
impl Entitlements for EntitlementService {
    async fn user(&self, user_id: i64) -> PayResult<UserEntitlement> {
        if let Some(u) = self.users.get(&user_id).await {
            // Time moves on inside the cache window: recompute the status around `now`.
            return Ok(u);
        }
        let cfg = self.config().await?;
        let patron = self
            .src
            .user_entitlement(user_id, (self.clock)(), cfg.grace_days)
            .await?;
        let u = UserEntitlement { user_id, patron };
        self.users.insert(user_id, u.clone()).await;
        Ok(u)
    }

    async fn guild(&self, guild_id: i64) -> PayResult<GuildEntitlement> {
        if let Some(g) = self.guilds.get(&guild_id).await {
            return Ok(g);
        }
        let g = self.src.guild_entitlement(guild_id, (self.clock)()).await?;
        self.guilds.insert(guild_id, g.clone()).await;
        Ok(g)
    }

    async fn limits(&self, user_id: i64, guild_id: Option<i64>) -> PayResult<Limits> {
        let cfg = self.config().await?;
        let patron = self.user(user_id).await?.patron.perks_on();
        let level = match guild_id {
            Some(g) => self.guild(g).await?.level,
            None => 0,
        };
        Ok(effective_limits(&cfg, patron, level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patron(id: i64, starts: i64, days: i64) -> Grant {
        Grant {
            id,
            kind: GrantKind::Patron,
            user_id: Some(1),
            guild_id: None,
            source: GrantSource::Invoice,
            source_id: Some(id),
            days: Some(days),
            ends_ms: None,
            permanent: false,
            usd_cents: None,
            starts_ms: starts,
            revoked_ms: None,
            reason: None,
        }
    }

    fn pool(id: i64, starts: i64, cents: i64) -> Grant {
        Grant {
            kind: GrantKind::ServerPool,
            user_id: Some(id),
            guild_id: Some(9),
            days: None,
            usd_cents: Some(cents),
            ..patron(id, starts, 0)
        }
    }

    #[test]
    fn patron_stacks_and_has_a_grace_period() {
        // 30 days paid on day 0, one more month paid on day 10: ends on day 60.
        let g = [patron(1, 0, 30), patron(2, 10 * DAY_MS, 30)];
        let span = stack_patron(&g);
        assert_eq!(span.until_ms, Some(60 * DAY_MS));
        assert_eq!(span.started_ms, Some(0));
        let s = patron_status(span, 59 * DAY_MS, 3);
        assert!(s.active && !s.in_grace && s.perks_on());
        let s = patron_status(span, 61 * DAY_MS, 3);
        assert!(!s.active && s.in_grace && s.perks_on());
        let s = patron_status(span, 63 * DAY_MS + 1, 3);
        assert!(!s.perks_on());
    }

    #[test]
    fn patron_after_a_gap_starts_at_the_payment() {
        let g = [patron(1, 0, 30), patron(2, 100 * DAY_MS, 30)];
        assert_eq!(stack_patron(&g).until_ms, Some(130 * DAY_MS));
        // Prepaying 12 months at once.
        assert_eq!(
            stack_patron(&[patron(1, 0, 360)]).until_ms,
            Some(360 * DAY_MS)
        );
    }

    #[test]
    fn revoked_grants_are_ignored_so_a_reorg_reverts_the_time() {
        let mut g = [patron(1, 0, 30), patron(2, 10 * DAY_MS, 30)];
        g[1].revoked_ms = Some(11 * DAY_MS);
        assert_eq!(stack_patron(&g).until_ms, Some(30 * DAY_MS));
        g[0].revoked_ms = Some(11 * DAY_MS);
        let span = stack_patron(&g);
        assert_eq!(span.until_ms, None);
        assert_eq!(patron_status(span, 12 * DAY_MS, 3), PatronStatus::none());
    }

    #[test]
    fn permanent_and_fixed_end_grants() {
        let mut perm = patron(1, 0, 0);
        perm.days = None;
        perm.permanent = true;
        let s = patron_status(stack_patron(&[perm]), 10_000 * DAY_MS, 3);
        assert!(s.active && s.permanent && s.until_ms.is_none());
        let mut fixed = patron(2, 5 * DAY_MS, 0);
        fixed.days = None;
        fixed.ends_ms = Some(20 * DAY_MS);
        let span = stack_patron(&[fixed.clone(), patron(3, 6 * DAY_MS, 30)]);
        // The 30 days stack after the fixed end (day 20).
        assert_eq!(span.until_ms, Some(50 * DAY_MS));
        assert_eq!(stack_patron(&[fixed]).until_ms, Some(20 * DAY_MS));
    }

    #[test]
    fn pool_window_and_levels() {
        let g = [
            pool(1, 0, 600),
            pool(2, 20 * DAY_MS, 500),
            pool(3, 25 * DAY_MS, 1500),
        ];
        // On day 31 the first contribution (day 0) left the 30-day window.
        assert_eq!(pool_cents(&g, 31 * DAY_MS, 30), 2000);
        assert_eq!(pool_cents(&g[..1], 10 * DAY_MS, 30), 600);
        let t = [1000, 2500, 5000];
        assert_eq!(level_for_pool(0, t), 0);
        assert_eq!(level_for_pool(999, t), 0);
        assert_eq!(level_for_pool(1000, t), 1);
        assert_eq!(level_for_pool(2499, t), 1);
        assert_eq!(level_for_pool(2500, t), 2);
        assert_eq!(level_for_pool(99_999, t), 3);
        let mut revoked = g.clone();
        revoked[2].revoked_ms = Some(1);
        assert_eq!(pool_cents(&revoked, 31 * DAY_MS, 30), 500);
    }

    #[test]
    fn level_drop_is_held_for_the_grace_period() {
        let t = [1000, 2500, 5000];
        // Level 2, the pool fell to $12: keep level 2, start the grace clock.
        let (l, b) = next_level(2, None, 1200, t, 3, 10 * DAY_MS);
        assert_eq!((l, b), (2, Some(10 * DAY_MS)));
        // Two days later: still held.
        let (l, b) = next_level(2, b, 1200, t, 3, 12 * DAY_MS);
        assert_eq!((l, b), (2, Some(10 * DAY_MS)));
        // After 3 days: downgrade to what the pool earns.
        let (l, b) = next_level(2, b, 1200, t, 3, 13 * DAY_MS);
        assert_eq!((l, b), (1, None));
        // Topping the pool up inside the grace period clears the clock.
        let (l, b) = next_level(2, Some(10 * DAY_MS), 3000, t, 3, 11 * DAY_MS);
        assert_eq!((l, b), (2, None));
        // Rising is immediate.
        assert_eq!(next_level(0, None, 5000, t, 3, 0), (3, None));
    }

    struct Src(std::sync::atomic::AtomicUsize);

    #[async_trait]
    impl EntitlementSource for Src {
        async fn user_entitlement(&self, _u: i64, _now: i64, _g: u32) -> PayResult<PatronStatus> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(PatronStatus {
                active: true,
                in_grace: false,
                permanent: false,
                until_ms: Some(1),
                started_ms: Some(0),
            })
        }

        async fn guild_entitlement(
            &self,
            guild_id: i64,
            _now_ms: i64,
        ) -> PayResult<GuildEntitlement> {
            Ok(GuildEntitlement {
                guild_id,
                level: 2,
                pool_cents: 3000,
                grace_until_ms: None,
            })
        }

        async fn config(&self) -> PayResult<DonationConfig> {
            Ok(DonationConfig::default())
        }
    }

    #[tokio::test]
    async fn service_caches_and_invalidates() {
        let src = Arc::new(Src(Default::default()));
        let svc = EntitlementService::new(src.clone(), Arc::new(|| 0), Duration::from_secs(60));
        let l = svc.limits(7, Some(9)).await.unwrap();
        assert!(l.patron);
        assert_eq!(l.guild_level, 2);
        assert_eq!(l.upload_bytes, 500 * crate::config::MIB);
        svc.limits(7, None).await.unwrap();
        assert_eq!(
            src.0.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "second lookup is cached"
        );
        svc.invalidate_user(7).await;
        svc.user(7).await.unwrap();
        assert_eq!(src.0.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(svc.limits(7, None).await.unwrap().guild_level, 0);
    }
}
