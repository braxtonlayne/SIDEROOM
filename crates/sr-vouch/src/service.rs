use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;

use crate::error::{VouchError, VouchResult};
use crate::tiers::{self, Tier};

/// Vouches one member may hold that are less than 24 hours old (revoking one frees a slot; cycling a single pair
/// adds nothing to anyone's count, so the cap only has to stop bulk vouching).
pub const VOUCHES_PER_DAY: i64 = 20;
/// Vouches one member may have outstanding at once (a Pillar needs 50 of them, an Elder 500, so no single
/// member can mint a crown on their own).
pub const MAX_VOUCHES_GIVEN: i64 = 500;

/// A member in good standing: active, not a bot, at least three days old (the wave 42 rule, so a fresh
/// throwaway account neither gives nor adds to a count).
macro_rules! good_standing {
    () => {
        "a.state::text = 'active' AND NOT a.is_bot AND a.created_at <= now() - make_interval(days => 3)"
    };
}

/// The private notice a voucher reads when a member they vouched for was banned for a serious category. It names
/// the category only, never the banned member's handle, and says plainly that nothing happens to the voucher.
pub fn fallout_notice(category: &str) -> String {
    format!(
        "A member you vouched for was banned for a serious rule violation ({category}). Your vouch for them was \
         removed. Nothing happens to your account: vouching carries no penalty."
    )
}

/// Delivers the private notice. The server implements it over the system DM (`SubjectNotifier::system_dm`).
#[async_trait]
pub trait VouchNotifier: Send + Sync {
    /// Tell `to` privately. `true` when it was delivered.
    async fn notify(&self, to: i64, text: &str) -> bool;
}

/// Whether an account may not vouch because inviting was taken from it (wave 44-A sterilization). With no check
/// wired, nobody is sterilized.
#[async_trait]
pub trait SterilizationCheck: Send + Sync {
    async fn is_sterilized(&self, account: i64) -> bool;
}

/// What the moderation engine asks (the `strike_weight_multiplier(account) -> f32` seam of wave 44-A, 1.0 default).
#[async_trait]
pub trait AccountabilityMultiplier: Send + Sync {
    /// 1.0 for a Citizen, 1.5 for a Pillar, 3.0 for an Elder. 1.0 when the lookup fails: a database error never
    /// makes a sanction heavier.
    async fn strike_weight_multiplier(&self, account: i64) -> f32;
}

/// What the vouch panel of a profile shows one viewer.
#[derive(Debug, Clone, PartialEq)]
pub struct VouchSummary {
    pub count: i64,
    pub tier: Tier,
    pub multiplier: f32,
    /// The viewer has an active vouch for this member.
    pub viewer_vouched: bool,
    /// The viewer may vouch for this member now (not themselves, in good standing, not sterilized, not already).
    pub can_vouch: bool,
}

/// What a serious ban did to the vouches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fallout {
    /// Vouches for the banned member that were removed.
    pub removed: usize,
    /// Of their vouchers, how many received the private notice.
    pub notified: usize,
}

/// The vouch handle over one database. Cheap to clone.
#[derive(Clone)]
pub struct Vouches {
    pool: PgPool,
    notifier: Option<Arc<dyn VouchNotifier>>,
    sterilized: Option<Arc<dyn SterilizationCheck>>,
}

impl Vouches {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            notifier: None,
            sterilized: None,
        }
    }

    pub fn with_notifier(mut self, n: Arc<dyn VouchNotifier>) -> Self {
        self.notifier = Some(n);
        self
    }

    pub fn with_sterilization_check(mut self, c: Arc<dyn SterilizationCheck>) -> Self {
        self.sterilized = Some(c);
        self
    }

    /// Vouches that count for `account`: from vouchers in good standing.
    pub async fn count(&self, account: i64) -> VouchResult<i64> {
        Ok(sqlx::query_scalar(concat!(
            "SELECT count(*) FROM profile_vouches v JOIN accounts a ON a.id = v.voucher_id \
             WHERE v.vouchee_id = $1 AND ",
            good_standing!()
        ))
        .bind(account)
        .fetch_one(&self.pool)
        .await?)
    }

    /// The tier of `account`.
    pub async fn tier(&self, account: i64) -> VouchResult<Tier> {
        Ok(tiers::tier_for(self.count(account).await?))
    }

    /// The accountability multiplier of `account`.
    pub async fn multiplier(&self, account: i64) -> VouchResult<f32> {
        Ok(tiers::multiplier_for(self.count(account).await?))
    }

    async fn may_give(&self, voucher: i64) -> VouchResult<bool> {
        let ok: bool = sqlx::query_scalar(concat!(
            "SELECT EXISTS (SELECT 1 FROM accounts a WHERE a.id = $1 AND ",
            good_standing!(),
            ")"
        ))
        .bind(voucher)
        .fetch_one(&self.pool)
        .await?;
        if !ok {
            return Ok(false);
        }
        if let Some(c) = &self.sterilized
            && c.is_sterilized(voucher).await
        {
            return Ok(false);
        }
        Ok(true)
    }

    async fn target_open(&self, voucher: i64, target: i64) -> VouchResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM accounts a WHERE a.id = $2 AND a.state::text = 'active' \
             AND NOT a.is_bot) AND NOT EXISTS (SELECT 1 FROM relationships WHERE type = 2 AND \
             ((user_id = $1 AND target_id = $2) OR (user_id = $2 AND target_id = $1)))",
        )
        .bind(voucher)
        .bind(target)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn has_vouched(&self, voucher: i64, target: i64) -> VouchResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM profile_vouches WHERE voucher_id = $1 AND vouchee_id = $2)",
        )
        .bind(voucher)
        .bind(target)
        .fetch_one(&self.pool)
        .await?)
    }

    /// `voucher` vouches for `target`. Idempotent: vouching again changes nothing and returns `false`; the first
    /// vouch returns `true`. Rules: not yourself; the target is an active member and neither side blocked the
    /// other (one "no such member" answer for both); the voucher is in good standing and not sterilized; at most
    /// [`VOUCHES_PER_DAY`] new vouches per day and [`MAX_VOUCHES_GIVEN`] outstanding.
    pub async fn vouch(&self, voucher: i64, target: i64) -> VouchResult<bool> {
        if voucher == target {
            return Err(VouchError::SelfVouch);
        }
        if !self.may_give(voucher).await? {
            return Err(VouchError::NotAllowed);
        }
        if !self.target_open(voucher, target).await? {
            return Err(VouchError::NotFound);
        }
        if self.has_vouched(voucher, target).await? {
            return Ok(false);
        }
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM profile_vouches WHERE voucher_id = $1 \
             AND created_at > now() - interval '24 hours'",
        )
        .bind(voucher)
        .fetch_one(&self.pool)
        .await?;
        if recent >= VOUCHES_PER_DAY {
            return Err(VouchError::RateLimited(
                "You have vouched for a lot of members today. Try again tomorrow.",
            ));
        }
        let given: i64 =
            sqlx::query_scalar("SELECT count(*) FROM profile_vouches WHERE voucher_id = $1")
                .bind(voucher)
                .fetch_one(&self.pool)
                .await?;
        if given >= MAX_VOUCHES_GIVEN {
            return Err(VouchError::RateLimited(
                "You have given the most vouches a member can. Revoke one first.",
            ));
        }
        let done = sqlx::query(
            "INSERT INTO profile_vouches (voucher_id, vouchee_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(voucher)
        .bind(target)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Withdraw the vouch. `true` when there was one. Always allowed, whatever the voucher's standing (a
    /// sterilized or blocked member can still take a vouch back).
    pub async fn revoke(&self, voucher: i64, target: i64) -> VouchResult<bool> {
        let done =
            sqlx::query("DELETE FROM profile_vouches WHERE voucher_id = $1 AND vouchee_id = $2")
                .bind(voucher)
                .bind(target)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The vouch panel of `target` as `viewer` sees it.
    pub async fn summary(&self, viewer: i64, target: i64) -> VouchResult<VouchSummary> {
        let count = self.count(target).await?;
        let viewer_vouched = self.has_vouched(viewer, target).await?;
        let can_vouch = viewer != target
            && !viewer_vouched
            && self.may_give(viewer).await?
            && self.target_open(viewer, target).await?;
        Ok(VouchSummary {
            count,
            tier: tiers::tier_for(count),
            multiplier: tiers::multiplier_for(count),
            viewer_vouched,
            can_vouch,
        })
    }

    /// The serious-ban subscriber: `banned` was banned for a serious `category` (wave 44-A emits it). Removes
    /// every vouch FOR them and sends each voucher the private notice. Vouchers get no freeze, no strike and no
    /// penalty of any kind: this function touches only `profile_vouches` and sends the notice. The vouches the
    /// banned member GAVE stay (they stop counting while the account is not active).
    pub async fn on_serious_ban(&self, banned: i64, category: &str) -> VouchResult<Fallout> {
        let vouchers: Vec<i64> = sqlx::query_scalar(
            "DELETE FROM profile_vouches WHERE vouchee_id = $1 RETURNING voucher_id",
        )
        .bind(banned)
        .fetch_all(&self.pool)
        .await?;
        let mut notified = 0;
        if let Some(n) = &self.notifier {
            let text = fallout_notice(category);
            for v in &vouchers {
                if n.notify(*v, &text).await {
                    notified += 1;
                } else {
                    tracing::warn!("could not deliver a vouch notice");
                }
            }
        }
        Ok(Fallout {
            removed: vouchers.len(),
            notified,
        })
    }

    /// GDPR erasure: every vouch the account gave or received. Returns how many rows went. (The account
    /// deletion transaction in `sr-db` runs the same statement; this is for the data-request path and tests.)
    pub async fn erase(&self, account: i64) -> VouchResult<u64> {
        let done =
            sqlx::query("DELETE FROM profile_vouches WHERE voucher_id = $1 OR vouchee_id = $1")
                .bind(account)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected())
    }
}

#[async_trait]
impl AccountabilityMultiplier for Vouches {
    async fn strike_weight_multiplier(&self, account: i64) -> f32 {
        match self.multiplier(account).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "vouch multiplier lookup failed, using 1.0");
                1.0
            }
        }
    }
}
