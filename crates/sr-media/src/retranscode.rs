//! Re-transcode planning (CIN-ING-011): single asset, whole title, or everything (admin).
//!
//! A pure function from the request and a snapshot of assets to the list of jobs to enqueue
//! and the assets skipped, with the reason for each skip so the admin UI can explain it.

use crate::retention::AssetState;
use serde::{Deserialize, Serialize};

/// Largest batch a single admin action may enqueue.
pub const MAX_BATCH: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "id", rename_all = "snake_case")]
pub enum RetranscodeScope {
    Asset(u64),
    Title(u64),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    /// Bulk work; yields to everything else.
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetranscodeRequest {
    pub scope: RetranscodeScope,
    /// Only assets rendered with an older profile than this (`None` = regardless).
    pub only_older_than_profile: Option<u32>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetSummary {
    pub asset_id: u64,
    pub title_id: u64,
    pub state: AssetState,
    pub has_original: bool,
    pub profile_version: u32,
    pub has_active_job: bool,
    pub duration_secs: f64,
    pub original_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    OriginalMissing,
    AlreadyQueued,
    NotPublishable,
    UpToDate,
    BatchLimit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetranscodePlan {
    pub priority: JobPriority,
    pub enqueue: Vec<u64>,
    pub skipped: Vec<(u64, SkipReason)>,
    /// Rough GPU time: 12 minutes per 2 hours of source (doc 08), i.e. duration / 10.
    pub estimated_gpu_secs: f64,
    pub original_bytes_to_read: u64,
    /// More assets matched than `MAX_BATCH`; run the action again for the rest.
    pub truncated: bool,
}

/// Plan the jobs for `req` over `assets`.
pub fn plan_retranscode(req: &RetranscodeRequest, assets: &[AssetSummary]) -> RetranscodePlan {
    let priority = match req.scope {
        RetranscodeScope::Asset(_) => JobPriority::High,
        RetranscodeScope::Title(_) => JobPriority::Normal,
        RetranscodeScope::All => JobPriority::Low,
    };
    let mut plan = RetranscodePlan {
        priority,
        enqueue: Vec::new(),
        skipped: Vec::new(),
        estimated_gpu_secs: 0.0,
        original_bytes_to_read: 0,
        truncated: false,
    };
    for a in assets {
        let in_scope = match req.scope {
            RetranscodeScope::Asset(id) => a.asset_id == id,
            RetranscodeScope::Title(id) => a.title_id == id,
            RetranscodeScope::All => true,
        };
        if !in_scope {
            // Not reported: only assets the request could have touched are listed.
            continue;
        }
        let skip = if !a.has_original {
            Some(SkipReason::OriginalMissing)
        } else if a.has_active_job {
            Some(SkipReason::AlreadyQueued)
        } else if !matches!(a.state, AssetState::Published | AssetState::Failed) {
            Some(SkipReason::NotPublishable)
        } else if req
            .only_older_than_profile
            .is_some_and(|v| a.profile_version >= v)
        {
            Some(SkipReason::UpToDate)
        } else {
            None
        };
        match skip {
            Some(r) => plan.skipped.push((a.asset_id, r)),
            None if plan.enqueue.len() >= MAX_BATCH => {
                plan.truncated = true;
                plan.skipped.push((a.asset_id, SkipReason::BatchLimit));
            }
            None => {
                plan.enqueue.push(a.asset_id);
                plan.estimated_gpu_secs += a.duration_secs / 10.0;
                plan.original_bytes_to_read += a.original_size;
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: u64, title: u64) -> AssetSummary {
        AssetSummary {
            asset_id: id,
            title_id: title,
            state: AssetState::Published,
            has_original: true,
            profile_version: 1,
            has_active_job: false,
            duration_secs: 7200.0,
            original_size: 30_000_000_000,
        }
    }

    fn req(scope: RetranscodeScope) -> RetranscodeRequest {
        RetranscodeRequest {
            scope,
            only_older_than_profile: None,
            reason: "new profile".into(),
        }
    }

    #[test]
    fn single_asset() {
        let assets = [asset(1, 10), asset(2, 10), asset(3, 11)];
        let p = plan_retranscode(&req(RetranscodeScope::Asset(2)), &assets);
        assert_eq!(p.enqueue, [2]);
        assert_eq!(p.priority, JobPriority::High);
        assert!(p.skipped.is_empty());
        assert!((p.estimated_gpu_secs - 720.0).abs() < 1e-6);
        assert_eq!(p.original_bytes_to_read, 30_000_000_000);
    }

    #[test]
    fn whole_title_covers_every_episode() {
        let assets = [asset(1, 10), asset(2, 10), asset(3, 11)];
        let p = plan_retranscode(&req(RetranscodeScope::Title(10)), &assets);
        assert_eq!(p.enqueue, [1, 2]);
        assert_eq!(p.priority, JobPriority::Normal);
    }

    #[test]
    fn all_is_low_priority_and_reports_skips() {
        let mut no_original = asset(2, 10);
        no_original.has_original = false;
        let mut queued = asset(3, 10);
        queued.has_active_job = true;
        let mut review = asset(4, 10);
        review.state = AssetState::InReview;
        let mut failed = asset(5, 10);
        failed.state = AssetState::Failed;
        let assets = [asset(1, 10), no_original, queued, review, failed];
        let p = plan_retranscode(&req(RetranscodeScope::All), &assets);
        assert_eq!(p.priority, JobPriority::Low);
        assert_eq!(p.enqueue, [1, 5]);
        assert_eq!(
            p.skipped,
            [
                (2, SkipReason::OriginalMissing),
                (3, SkipReason::AlreadyQueued),
                (4, SkipReason::NotPublishable)
            ]
        );
    }

    #[test]
    fn only_outdated_profiles() {
        let mut old = asset(1, 10);
        old.profile_version = 1;
        let mut new = asset(2, 10);
        new.profile_version = 2;
        let r = RetranscodeRequest {
            only_older_than_profile: Some(2),
            ..req(RetranscodeScope::All)
        };
        let p = plan_retranscode(&r, &[old, new]);
        assert_eq!(p.enqueue, [1]);
        assert_eq!(p.skipped, [(2, SkipReason::UpToDate)]);
    }

    #[test]
    fn batches_are_capped() {
        let assets: Vec<_> = (0..(MAX_BATCH as u64 + 3)).map(|i| asset(i, 1)).collect();
        let p = plan_retranscode(&req(RetranscodeScope::All), &assets);
        assert_eq!(p.enqueue.len(), MAX_BATCH);
        assert!(p.truncated);
        assert_eq!(p.skipped.len(), 3);
        assert!(p.skipped.iter().all(|(_, r)| *r == SkipReason::BatchLimit));
    }

    #[test]
    fn unknown_asset_yields_empty_plan() {
        let p = plan_retranscode(&req(RetranscodeScope::Asset(99)), &[asset(1, 10)]);
        assert!(p.enqueue.is_empty() && p.skipped.is_empty());
    }
}
