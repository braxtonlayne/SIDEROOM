//! Originals retention and proxy cleanup (CIN-ING-009), as pure decisions over asset facts.
//!
//! Originals are the only copy that can be re-transcoded (renditions are not backed up), so
//! the default is conservative: keep them, move them from the hot bucket to the cold bucket
//! (Storage Box or Garage cold) some time after publishing, and delete only after a title is
//! removed or a submission is rejected plus a grace period. The review proxy is deleted soon
//! after the ladder is published.

use serde::{Deserialize, Serialize};

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// `false` deletes originals after publish (storage-constrained instances; re-transcoding
    /// then needs a re-upload).
    pub keep_originals: bool,
    /// Move originals to the cold tier this many days after publishing. `None` = stay hot.
    pub cold_after_days: Option<u32>,
    /// Grace before originals of removed or rejected titles are deleted.
    pub delete_after_removal_days: u32,
    /// Keep the proxy this long after publishing (late reviewer follow-ups).
    pub proxy_grace_hours: u32,
    /// Proxies of assets stuck in any other state are deleted after this many days.
    pub stale_proxy_days: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            keep_originals: true,
            cold_after_days: Some(30),
            delete_after_removal_days: 30,
            proxy_grace_hours: 24,
            stale_proxy_days: 14,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetState {
    Uploaded,
    Probing,
    ProxyReady,
    InReview,
    Approved,
    Transcoding,
    Published,
    Rejected,
    Removed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Hot,
    Cold,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFacts {
    pub asset_id: u64,
    pub state: AssetState,
    pub tier: Tier,
    pub has_original: bool,
    pub original_size: u64,
    pub published_at: Option<u64>,
    /// When the asset was rejected or its title removed.
    pub ended_at: Option<u64>,
    pub has_proxy: bool,
    pub proxy_created_at: Option<u64>,
    /// A transcode job is queued or running for this asset.
    pub has_active_job: bool,
    /// A re-transcode has been requested but not yet queued.
    pub retranscode_pending: bool,
    /// Staff enabled original downloads for the title: keep it hot.
    pub original_downloads_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteReason {
    PolicyDoesNotKeepOriginals,
    TitleRemoved,
    SubmissionRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "reason", rename_all = "snake_case")]
pub enum OriginalAction {
    Keep,
    MoveToCold,
    Delete(DeleteReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyReason {
    Published,
    Rejected,
    Removed,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "reason", rename_all = "snake_case")]
pub enum ProxyAction {
    Keep,
    Delete(ProxyReason),
}

fn elapsed(since: Option<u64>, now: u64) -> Option<u64> {
    since.map(|t| now.saturating_sub(t))
}

/// What to do with the asset's original file.
pub fn decide_original(policy: &RetentionPolicy, a: &AssetFacts, now: u64) -> OriginalAction {
    if !a.has_original || a.has_active_job || a.retranscode_pending {
        return OriginalAction::Keep;
    }
    let grace = u64::from(policy.delete_after_removal_days) * DAY;
    match a.state {
        AssetState::Removed | AssetState::Rejected => {
            let reason = if a.state == AssetState::Removed {
                DeleteReason::TitleRemoved
            } else {
                DeleteReason::SubmissionRejected
            };
            // No end timestamp recorded: be safe and keep.
            match elapsed(a.ended_at, now) {
                Some(e) if e >= grace => OriginalAction::Delete(reason),
                _ => OriginalAction::Keep,
            }
        }
        AssetState::Published => {
            let since = elapsed(a.published_at, now);
            if !policy.keep_originals {
                return match since {
                    Some(e) if e >= u64::from(policy.proxy_grace_hours) * HOUR => {
                        OriginalAction::Delete(DeleteReason::PolicyDoesNotKeepOriginals)
                    }
                    _ => OriginalAction::Keep,
                };
            }
            match (a.tier, policy.cold_after_days, since) {
                (Tier::Hot, Some(days), Some(e))
                    if e >= u64::from(days) * DAY && !a.original_downloads_enabled =>
                {
                    OriginalAction::MoveToCold
                }
                _ => OriginalAction::Keep,
            }
        }
        // Mid-pipeline and failed assets keep their original.
        _ => OriginalAction::Keep,
    }
}

/// What to do with the asset's review proxy.
pub fn decide_proxy(policy: &RetentionPolicy, a: &AssetFacts, now: u64) -> ProxyAction {
    if !a.has_proxy {
        return ProxyAction::Keep;
    }
    match a.state {
        AssetState::Published => match elapsed(a.published_at, now) {
            Some(e) if e >= u64::from(policy.proxy_grace_hours) * HOUR => {
                ProxyAction::Delete(ProxyReason::Published)
            }
            _ => ProxyAction::Keep,
        },
        AssetState::Rejected => ProxyAction::Delete(ProxyReason::Rejected),
        AssetState::Removed => ProxyAction::Delete(ProxyReason::Removed),
        _ => match elapsed(a.proxy_created_at, now) {
            Some(e) if e >= u64::from(policy.stale_proxy_days) * DAY && !a.has_active_job => {
                ProxyAction::Delete(ProxyReason::Stale)
            }
            _ => ProxyAction::Keep,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub move_to_cold: Vec<u64>,
    pub delete_originals: Vec<(u64, DeleteReason)>,
    pub delete_proxies: Vec<(u64, ProxyReason)>,
    /// Bytes the deletions and moves take off the hot tier.
    pub hot_bytes_freed: u64,
}

/// The periodic retention sweep over a batch of assets.
pub fn plan_cleanup(policy: &RetentionPolicy, assets: &[AssetFacts], now: u64) -> CleanupPlan {
    let mut plan = CleanupPlan::default();
    for a in assets {
        match decide_original(policy, a, now) {
            OriginalAction::Keep => {}
            OriginalAction::MoveToCold => {
                plan.move_to_cold.push(a.asset_id);
                plan.hot_bytes_freed += a.original_size;
            }
            OriginalAction::Delete(r) => {
                plan.delete_originals.push((a.asset_id, r));
                if a.tier == Tier::Hot {
                    plan.hot_bytes_freed += a.original_size;
                }
            }
        }
        if let ProxyAction::Delete(r) = decide_proxy(policy, a, now) {
            plan.delete_proxies.push((a.asset_id, r));
        }
    }
    plan
}

/// Disk pressure on the hot tier: pick published, idle, hot originals to move to cold,
/// oldest publication first, until `need_bytes` would be freed.
pub fn plan_hot_reclaim(assets: &[AssetFacts], need_bytes: u64) -> Vec<u64> {
    let mut candidates: Vec<&AssetFacts> = assets
        .iter()
        .filter(|a| {
            a.has_original
                && a.tier == Tier::Hot
                && a.state == AssetState::Published
                && !a.has_active_job
                && !a.retranscode_pending
                && !a.original_downloads_enabled
        })
        .collect();
    candidates.sort_by_key(|a| (a.published_at.unwrap_or(u64::MAX), a.asset_id));
    let mut freed = 0u64;
    let mut out = Vec::new();
    for a in candidates {
        if freed >= need_bytes {
            break;
        }
        freed += a.original_size;
        out.push(a.asset_id);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1000 * DAY;

    fn facts(state: AssetState) -> AssetFacts {
        AssetFacts {
            asset_id: 1,
            state,
            tier: Tier::Hot,
            has_original: true,
            original_size: 10_000,
            published_at: None,
            ended_at: None,
            has_proxy: true,
            proxy_created_at: Some(NOW - DAY),
            has_active_job: false,
            retranscode_pending: false,
            original_downloads_enabled: false,
        }
    }

    #[test]
    fn published_original_moves_to_cold_after_the_delay() {
        let p = RetentionPolicy::default();
        let mut a = facts(AssetState::Published);
        a.published_at = Some(NOW - 29 * DAY);
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
        a.published_at = Some(NOW - 30 * DAY);
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::MoveToCold);
        a.tier = Tier::Cold;
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
    }

    #[test]
    fn original_downloads_pin_the_original_hot() {
        let mut a = facts(AssetState::Published);
        a.published_at = Some(NOW - 90 * DAY);
        a.original_downloads_enabled = true;
        assert_eq!(
            decide_original(&RetentionPolicy::default(), &a, NOW),
            OriginalAction::Keep
        );
    }

    #[test]
    fn cold_disabled_stays_hot() {
        let p = RetentionPolicy {
            cold_after_days: None,
            ..RetentionPolicy::default()
        };
        let mut a = facts(AssetState::Published);
        a.published_at = Some(NOW - 900 * DAY);
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
    }

    #[test]
    fn removed_and_rejected_delete_after_grace() {
        let p = RetentionPolicy::default();
        let mut a = facts(AssetState::Removed);
        a.ended_at = Some(NOW - 29 * DAY);
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
        a.ended_at = Some(NOW - 31 * DAY);
        assert_eq!(
            decide_original(&p, &a, NOW),
            OriginalAction::Delete(DeleteReason::TitleRemoved)
        );
        let mut r = facts(AssetState::Rejected);
        r.ended_at = Some(NOW - 31 * DAY);
        assert_eq!(
            decide_original(&p, &r, NOW),
            OriginalAction::Delete(DeleteReason::SubmissionRejected)
        );
        // No timestamp recorded: never delete.
        r.ended_at = None;
        assert_eq!(decide_original(&p, &r, NOW), OriginalAction::Keep);
    }

    #[test]
    fn active_jobs_and_pending_retranscodes_protect_the_original() {
        let p = RetentionPolicy::default();
        let mut a = facts(AssetState::Removed);
        a.ended_at = Some(NOW - 90 * DAY);
        a.has_active_job = true;
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
        a.has_active_job = false;
        a.retranscode_pending = true;
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
    }

    #[test]
    fn mid_pipeline_and_failed_keep_original() {
        let p = RetentionPolicy::default();
        for s in [
            AssetState::Uploaded,
            AssetState::InReview,
            AssetState::Transcoding,
            AssetState::Failed,
        ] {
            assert_eq!(decide_original(&p, &facts(s), NOW), OriginalAction::Keep);
        }
    }

    #[test]
    fn not_keeping_originals_deletes_after_publish_grace() {
        let p = RetentionPolicy {
            keep_originals: false,
            ..RetentionPolicy::default()
        };
        let mut a = facts(AssetState::Published);
        a.published_at = Some(NOW - HOUR);
        assert_eq!(decide_original(&p, &a, NOW), OriginalAction::Keep);
        a.published_at = Some(NOW - 25 * HOUR);
        assert_eq!(
            decide_original(&p, &a, NOW),
            OriginalAction::Delete(DeleteReason::PolicyDoesNotKeepOriginals)
        );
    }

    #[test]
    fn proxy_cleanup_rules() {
        let p = RetentionPolicy::default();
        let mut a = facts(AssetState::Published);
        a.published_at = Some(NOW - 2 * HOUR);
        assert_eq!(decide_proxy(&p, &a, NOW), ProxyAction::Keep);
        a.published_at = Some(NOW - 25 * HOUR);
        assert_eq!(
            decide_proxy(&p, &a, NOW),
            ProxyAction::Delete(ProxyReason::Published)
        );
        assert_eq!(
            decide_proxy(&p, &facts(AssetState::Rejected), NOW),
            ProxyAction::Delete(ProxyReason::Rejected)
        );
        assert_eq!(
            decide_proxy(&p, &facts(AssetState::Removed), NOW),
            ProxyAction::Delete(ProxyReason::Removed)
        );
        // In review with a fresh proxy: keep. Stale proxy with no job: delete.
        let mut r = facts(AssetState::InReview);
        assert_eq!(decide_proxy(&p, &r, NOW), ProxyAction::Keep);
        r.proxy_created_at = Some(NOW - 15 * DAY);
        assert_eq!(
            decide_proxy(&p, &r, NOW),
            ProxyAction::Delete(ProxyReason::Stale)
        );
        r.has_active_job = true;
        assert_eq!(decide_proxy(&p, &r, NOW), ProxyAction::Keep);
        let mut none = facts(AssetState::Published);
        none.has_proxy = false;
        none.published_at = Some(0);
        assert_eq!(decide_proxy(&p, &none, NOW), ProxyAction::Keep);
    }

    #[test]
    fn sweep_totals() {
        let p = RetentionPolicy::default();
        let mut cold = facts(AssetState::Published);
        cold.asset_id = 1;
        cold.published_at = Some(NOW - 40 * DAY);
        let mut gone = facts(AssetState::Removed);
        gone.asset_id = 2;
        gone.ended_at = Some(NOW - 40 * DAY);
        gone.original_size = 5_000;
        let mut idle = facts(AssetState::InReview);
        idle.asset_id = 3;
        let plan = plan_cleanup(&p, &[cold, gone, idle], NOW);
        assert_eq!(plan.move_to_cold, [1]);
        assert_eq!(plan.delete_originals, [(2, DeleteReason::TitleRemoved)]);
        assert_eq!(
            plan.delete_proxies,
            [(1, ProxyReason::Published), (2, ProxyReason::Removed)]
        );
        assert_eq!(plan.hot_bytes_freed, 15_000);
    }

    #[test]
    fn hot_reclaim_takes_oldest_first_and_stops() {
        let mk = |id, published, size| AssetFacts {
            asset_id: id,
            published_at: Some(published),
            original_size: size,
            ..facts(AssetState::Published)
        };
        let mut pinned = mk(9, 1, 1_000_000);
        pinned.original_downloads_enabled = true;
        let mut busy = mk(8, 2, 1_000_000);
        busy.has_active_job = true;
        let assets = [mk(3, 300, 40), mk(1, 100, 40), mk(2, 200, 40), pinned, busy];
        assert_eq!(plan_hot_reclaim(&assets, 50), [1, 2]);
        assert_eq!(plan_hot_reclaim(&assets, 0), Vec::<u64>::new());
        assert_eq!(plan_hot_reclaim(&assets, 1_000), [1, 2, 3]);
    }
}
