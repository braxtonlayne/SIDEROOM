//! `arcade.manifest_recheck` (D-579, SEC-AUD-045): a one-off boot job that brings guild cartridges listed before
//! D-568 under it. Each row never checked against its file's `sideroom.json` has the manifest read through the
//! shelf reader a listing uses, and its tier and capabilities rewritten from it; a row whose file cannot be read,
//! has no manifest or is not a zip is delisted and staff get a `cartridge_delisted` alert naming it, so the lister
//! can list it again. Singleton with a reserved id, armed at every boot; with nothing left to check it ends at once.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sr_activities::arcade::{ArcadeService, CartridgeObjects, DelistedCartridge};
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_events::{DomainEvent, EventBus, StaffAlertKind, Topic};

/// Reserved singleton id for `arcade.manifest_recheck` (the number of its migration).
pub const RECHECK_JOB_ID: i64 = 2440;
/// Kind of the one-off re-check.
pub const RECHECK_KIND: &str = "arcade.manifest_recheck";
/// Delay after boot, so the server is serving before the shelf reads start.
const BOOT_DELAY: Duration = Duration::from_secs(60);
/// Rows read per pass.
const BATCH: i64 = 50;

/// The alert text for a delisted cartridge.
pub fn alert_detail(c: &DelistedCartridge) -> String {
    let lister = if c.added_by_name.is_empty() {
        format!("account {}", c.added_by)
    } else {
        format!("@{} ({})", c.added_by_name, c.added_by)
    };
    format!(
        "Cartridge \"{}\" ({}) on server \"{}\" ({}), listed by {lister}, was delisted: its file {} {} \
         The lister can list it again once the file has a sideroom.json that declares its tier.",
        c.title, c.id, c.guild_name, c.guild_id, c.source_ref, c.reason
    )
}

/// Run the re-check to the end: passes until none is left, one staff alert per delisted cartridge.
pub async fn run(
    arcade: &ArcadeService,
    objects: &dyn CartridgeObjects,
    bus: &Arc<dyn EventBus>,
) -> Result<(usize, usize), HandlerError> {
    let (mut rewritten, mut delisted) = (0, 0);
    loop {
        let pass = arcade.recheck_cartridges(objects, BATCH).await?;
        rewritten += pass.rewritten;
        delisted += pass.delisted.len();
        for c in &pass.delisted {
            tracing::warn!(cartridge = c.id, guild = c.guild_id, reason = %c.reason, "cartridge delisted");
            let ev = DomainEvent::StaffAlert {
                kind: StaffAlertKind::CartridgeDelisted,
                detail: alert_detail(c),
            };
            if let Err(e) = bus.publish(Topic::Platform, ev).await {
                tracing::warn!(error = %e, "could not publish a cartridge alert");
            }
        }
        if pass.checked == 0 {
            return Ok((rewritten, delisted));
        }
    }
}

/// Register the re-check on `runner`. Pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    arcade: ArcadeService,
    objects: Arc<dyn CartridgeObjects>,
    bus: Arc<dyn EventBus>,
) -> JobRunner {
    runner.register(RECHECK_KIND, 1, move |_job: Job| {
        let arcade = arcade.clone();
        let objects = objects.clone();
        let bus = bus.clone();
        Box::pin(async move {
            let (rewritten, delisted) = run(&arcade, &*objects, &bus).await?;
            if rewritten + delisted > 0 {
                tracing::info!(rewritten, delisted, "cartridge manifests re-checked");
            }
            Ok(())
        })
    })
}

/// Arm the re-check (again at every boot; it ends at once when every row is checked). Call only when
/// [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, RECHECK_JOB_ID, RECHECK_KIND, &json!({}), BOOT_DELAY).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use sr_db::accounts::{self, NewAccount};
    use sr_db::testing::TestDb;
    use sr_events::InProcessBus;
    use std::sync::atomic::{AtomicI64, Ordering};

    static SEQ: AtomicI64 = AtomicI64::new(1);

    fn uid() -> i64 {
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0);
        base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
    }

    /// No shelf has any file.
    struct NoFiles;

    #[async_trait::async_trait]
    impl CartridgeObjects for NoFiles {
        async fn read(&self, _shelf: i64, _path: &str, _len: u64) -> Result<Vec<u8>, String> {
            Err("is not a file on that shelf.".to_owned())
        }
    }

    #[test]
    fn the_alert_names_the_cartridge_its_server_and_lister() {
        let d = alert_detail(&DelistedCartridge {
            id: 7,
            guild_id: 9,
            guild_name: "Night Owls".into(),
            title: "Pong".into(),
            source_ref: "games/pong.zip".into(),
            added_by: 3,
            added_by_name: "ada".into(),
            reason: "has no sideroom.json.".into(),
        });
        for part in [
            "\"Pong\" (7)",
            "\"Night Owls\" (9)",
            "@ada (3)",
            "games/pong.zip has no",
        ] {
            assert!(d.contains(part), "{d}");
        }
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn an_unreadable_old_cartridge_is_delisted_with_one_staff_alert() {
        let db = TestDb::new().await.unwrap();
        let pool = db.pool();
        let user = uid();
        let name = format!("t{user}");
        assert!(
            accounts::create_account(
                pool,
                &NewAccount {
                    id: user,
                    person_id: uid(),
                    username: &name,
                    display_name: None,
                    email: &format!("{name}@example.test"),
                    password_hash: "x",
                    state: "active",
                    email_verified: true,
                    initial_credits: 0,
                    flags: 0,
                },
                uid(),
            )
            .await
            .unwrap()
        );
        let guild = uid();
        sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Night Owls', $2)")
            .bind(guild)
            .bind(user)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO arcade_guild_cartridges (id, guild_id, title, kind, tier, source_ref, added_by) \
             VALUES ($1, $2, 'Lost Game', 'game', 'TIER_1_ORIGINAL', 'games/lost.zip', $3)",
        )
        .bind(uid())
        .bind(guild)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
        let arcade = ArcadeService::new(pool.clone(), Arc::new(uid));
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let mut alerts = bus.subscribe(Topic::Platform);

        assert_eq!(run(&arcade, &NoFiles, &bus).await.unwrap(), (0, 1));
        let ev = tokio::time::timeout(Duration::from_secs(2), alerts.next())
            .await
            .unwrap()
            .unwrap();
        let DomainEvent::StaffAlert { kind, detail } = ev else {
            panic!("expected a staff alert");
        };
        assert_eq!(kind, StaffAlertKind::CartridgeDelisted);
        assert!(
            detail.contains("\"Lost Game\"") && detail.contains(&format!("@{name}")),
            "{detail}"
        );
        assert!(arcade.cartridges(guild).await.unwrap().is_empty());
        // Done once: a second run (the next boot) checks nothing and alerts nobody.
        assert_eq!(run(&arcade, &NoFiles, &bus).await.unwrap(), (0, 0));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), alerts.next())
                .await
                .is_err()
        );
        db.cleanup().await.unwrap();
    }
}
