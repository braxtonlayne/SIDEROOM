//! The takedown register's housekeeping (MOD-TKD-006, D-293) and the settings it reads.
//!
//! * `moderation.takedown_restore` (hourly): restores the content of every notice whose counter-notice wait
//!   (ten business days) is over, unless staff noted a court filing. Each restore is a logged action
//!   ([`sr_moderation::Moderation::run_due_restores`]).
//! * [`TakedownPolicySource`] hands the strike threshold, the window, the operator country and contact (the
//!   `compliance` feature module, Admin > Features) to the register.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_features::{COMPLIANCE_ID, ComplianceSettings, FeatureService, ScopeRef};
use sr_moderation::{Moderation, PolicySource, model::TakedownPolicy};

/// Kind of the restore job.
pub const RESTORE_KIND: &str = "moderation.takedown_restore";
/// Reserved singleton id (below any Snowflake; unique in this repository, see the grep in the handoff).
pub const RESTORE_JOB_ID: i64 = 1120;
const _: () = assert!(
    RESTORE_JOB_ID < 1 << 40,
    "singleton ids stay below the snowflakes"
);
pub const RESTORE_INTERVAL: Duration = Duration::from_secs(3600);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

async fn restore_handler(
    pool: PgPool,
    moderation: Moderation,
    job: Job,
) -> Result<(), HandlerError> {
    moderation
        .run_due_restores(now_ms())
        .await
        .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })?;
    jobs::reschedule_running(&pool, job.id, RESTORE_INTERVAL).await?;
    Ok(())
}

/// Register `moderation.takedown_restore`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, moderation: Moderation) -> JobRunner {
    let pool = pool.clone();
    runner.register(RESTORE_KIND, 1, move |job| {
        Box::pin(restore_handler(pool.clone(), moderation.clone(), job))
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        RESTORE_JOB_ID,
        RESTORE_KIND,
        &json!({}),
        Duration::ZERO,
    )
    .await?;
    Ok(())
}

/// Reads the strike policy and the operator details from the `compliance` feature module.
pub struct TakedownPolicySource {
    features: FeatureService,
}

impl TakedownPolicySource {
    pub fn new(features: FeatureService) -> Self {
        Self { features }
    }
}

#[async_trait]
impl PolicySource for TakedownPolicySource {
    async fn takedown_policy(&self) -> TakedownPolicy {
        let values = self
            .features
            .effective_settings(COMPLIANCE_ID, &ScopeRef::Platform)
            .await
            .unwrap_or_default();
        let s = ComplianceSettings::from_values(&values);
        TakedownPolicy {
            strike_review_threshold: u32::from(s.strike_review_threshold),
            strike_window_months: u32::from(s.strike_window_months),
            operator_country: s.operator_country,
            operator_contact: s.operator_contact,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_features::{FeatureRegistry, InMemorySettingsStore};
    use std::sync::Arc;

    #[tokio::test]
    async fn the_policy_follows_the_compliance_settings() {
        let mut r = FeatureRegistry::new();
        r.register(sr_features::ComplianceFeature).unwrap();
        let svc = FeatureService::new(Arc::new(r), Arc::new(InMemorySettingsStore::new()), None);
        let p = TakedownPolicySource::new(svc).takedown_policy().await;
        assert_eq!(p.strike_review_threshold, 3);
        assert_eq!(p.strike_window_months, 12);
        assert_eq!(p.operator_country, None);
    }
}

/// MOD-TKD-004 retention guard: the purge never takes a message a takedown holds, and takes it at the next run
/// once the takedown ends and the hold is released.
#[cfg(test)]
mod retention_tests {
    use super::*;
    use sr_db::testing::TestDb;
    use sr_moderation::model::{ItemKind, NewItem, NewNotice, TakedownKind, Who};

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_takedown_hold_blocks_the_purge_until_the_notice_ends() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        for (id, name) in [(1_i64, "author"), (2, "staff")] {
            sqlx::query("INSERT INTO persons (id) VALUES ($1)")
                .bind(id)
                .execute(pool)
                .await
                .expect("person");
            sqlx::query(
                "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
            )
            .bind(id)
            .bind(name)
            .bind(format!("{name}@example.test"))
            .execute(pool)
            .await
            .expect("account");
        }
        sqlx::query("INSERT INTO channels (id, type) VALUES (10, 1)")
            .execute(pool)
            .await
            .expect("channel");
        let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
            .fetch_one(pool)
            .await
            .expect("partition");
        let msg: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
                .fetch_one(pool)
                .await
                .expect("floor");
        // The author deleted it long ago: it is due for purge.
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content, visibility, deleted_at, purge_after) \
             VALUES ($1, 10, 1, 'x', 'user_deleted', now() - interval '90 days', now() - interval '60 days')",
        )
        .bind(msg)
        .execute(pool)
        .await
        .expect("message");

        let ids = std::sync::atomic::AtomicI64::new(8_000_000);
        let moderation = Moderation::new(
            pool.clone(),
            std::sync::Arc::new(move || ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
        );
        let staff = sr_admin::Caller {
            user_id: 2,
            permissions: sr_core::PlatformPermissions::MANAGE_TAKEDOWNS,
            is_owner: false,
            ip: None,
        };
        let who = Who::Staff(&staff);
        let notice = moderation
            .create_notice(
                &who,
                &NewNotice {
                    kind: TakedownKind::IllegalContent,
                    notifier_name: "Authority".into(),
                    notifier_contact: "a@b.example".into(),
                    legal_basis: "Court order".into(),
                    locations: vec![],
                    original_text: "Remove it".into(),
                    received_at_ms: None,
                },
            )
            .await
            .expect("notice")
            .summary
            .id;
        moderation
            .add_item(
                &who,
                notice,
                &NewItem {
                    kind: ItemKind::Message,
                    id: msg,
                },
            )
            .await
            .expect("item");
        moderation
            .hold_notice(&who, notice, Some("preserve"))
            .await
            .expect("hold");

        let storage = sr_storage::ObjectStoreStorage::in_memory();
        let later = now_ms() + 1000;
        assert_eq!(
            crate::jobs::purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            0,
            "held by the takedown: never purged"
        );
        // The notifier withdraws: the hold is released and the next purge run takes the message.
        moderation
            .withdraw_notice(&who, notice, Some("notifier retracted"))
            .await
            .expect("withdraw");
        assert_eq!(
            crate::jobs::purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            1,
            "released: purged at the next run"
        );
        db.cleanup().await.expect("cleanup");
    }
}
