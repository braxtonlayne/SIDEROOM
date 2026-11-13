//! `arcade.manifest_recheck` (D-579, SEC-AUD-045): a one-off boot job that brings guild cartridges listed before
//! D-568 under it. Each row never checked against its file's `sideroom.json` has the manifest read through the
//! shelf reader a listing uses, and its tier and capabilities rewritten from it; a row whose file cannot be read,
//! has no manifest or is not a zip is delisted and staff get a `cartridge_delisted` alert naming it, so the lister
//! can list it again. Singleton with a reserved id, armed at every boot; with nothing left to check it ends at once.
//!
//! D-582 (SEC-AUD-052): a definite answer (no mount, path not on the shelf, object not found, not a zip, no
//! manifest, no tier) delists; a transient one (a backend error, an unbuildable client, a failed endpoint check)
//! defers: the row stays unchecked, nothing is delisted, the job runs again every hour while any row is deferred,
//! and staff get one alert per shelf once its rows have been deferred for 24 hours. Each delisted cartridge's
//! lister gets one system DM and the server's audit log an `arcade.cartridge_delisted` entry (written with the
//! delisting, in the service).

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sr_activities::arcade::{
    ArcadeService, CartridgeObjects, DEFER_ALERT_HOURS, DEFER_DELIST_DAYS, DeferredShelf,
    DelistedCartridge,
};
use sr_api::routes::developer::{SystemCard, SystemDm};
use sr_api::routes::review::DmSlot;
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
/// How soon the job runs again while any cartridge is deferred (D-582).
pub const DEFER_RETRY: Duration = Duration::from_secs(3600);

/// The alert text for a delisted cartridge.
pub fn alert_detail(c: &DelistedCartridge) -> String {
    let lister = if c.added_by_name.is_empty() {
        format!("account {}", c.added_by)
    } else {
        format!("@{} ({})", c.added_by_name, c.added_by)
    };
    format!(
        "Cartridge \"{}\" ({}) on server \"{}\" ({}), listed by {lister}, was delisted: {} {}",
        c.title,
        c.id,
        c.guild_name,
        c.guild_id,
        delist_clause(c),
        if c.aged {
            "The lister can list it again once the shelf is answering again."
        } else {
            "The lister can list it again once the file has a sideroom.json that declares its tier."
        }
    )
}

/// What follows "was delisted: " in the staff alert: the source path with the reason, or the aged clause (D-590).
fn delist_clause(c: &DelistedCartridge) -> String {
    if c.aged {
        format!(
            "{} (file {}).",
            c.reason.trim_end_matches('.'),
            c.source_ref
        )
    } else {
        format!("its file {} {}", c.source_ref, c.reason)
    }
}

/// The staff alert for a shelf that has kept cartridges deferred for a day (D-582).
pub fn deferred_detail(d: &DeferredShelf) -> String {
    let servers = d.guild_names.join(", ");
    format!(
        "Shelf {} has not answered for {DEFER_ALERT_HOURS} hours, so {} server cartridge(s) ({servers}) \
         could not be re-checked against their sideroom.json. Nothing was delisted yet; the re-check tries \
         again every hour. If the shelf still has not answered, they are delisted on {} UTC ({DEFER_DELIST_DAYS} \
         days after the first failed check). Last reason: its file {}",
        d.shelf_id, d.cartridges, d.delist_on, d.reason
    )
}

/// The DM a delisted cartridge's lister gets (D-582): the cartridge, the server, why, and what to do.
pub fn lister_dm_text(c: &DelistedCartridge) -> String {
    format!(
        "Your cartridge \"{}\" on the server \"{}\" was delisted: {} List it again once {}.",
        c.title,
        c.guild_name,
        c.why(),
        if c.aged {
            "the shelf is answering again"
        } else {
            "the file is fixed"
        }
    )
}

/// What one run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    pub rewritten: usize,
    pub delisted: usize,
    pub deferred: usize,
    /// Shelves staff were told about.
    pub shelves_alerted: usize,
}

/// Run the re-check to the end: passes until none is left, one staff alert per delisted cartridge, one system DM to
/// each lister, one staff alert per shelf deferred for 24 hours. `run` names this run (a row deferred in it is not
/// looked at again by its later passes).
pub async fn run(
    arcade: &ArcadeService,
    objects: &dyn CartridgeObjects,
    bus: &Arc<dyn EventBus>,
    dm: Option<&dyn SystemDm>,
    run: i64,
) -> Result<Outcome, HandlerError> {
    let mut out = Outcome::default();
    loop {
        let pass = arcade.recheck_cartridges(objects, BATCH, run).await?;
        out.rewritten += pass.rewritten;
        out.delisted += pass.delisted.len();
        out.deferred += pass.deferred;
        for c in &pass.delisted {
            tracing::warn!(cartridge = c.id, guild = c.guild_id, reason = %c.reason, "cartridge delisted");
            let ev = DomainEvent::StaffAlert {
                kind: StaffAlertKind::CartridgeDelisted,
                detail: alert_detail(c),
            };
            if let Err(e) = bus.publish(Topic::Platform, ev).await {
                tracing::warn!(error = %e, "could not publish a cartridge alert");
            }
            match dm {
                Some(dm) => {
                    let card = SystemCard {
                        content: lister_dm_text(c),
                        ..SystemCard::default()
                    };
                    if !dm.dm(c.added_by, card).await {
                        tracing::warn!(
                            cartridge = c.id,
                            lister = c.added_by,
                            "delisting DM not delivered"
                        );
                    }
                }
                None => tracing::debug!("no system DM sender wired; delisting DM skipped"),
            }
        }
        // A pass that looked at nothing is the end; rows deferred in this run are skipped by the next query.
        if pass.checked == 0 {
            break;
        }
    }
    for shelf in arcade.stale_deferred_shelves().await? {
        out.shelves_alerted += 1;
        tracing::warn!(
            shelf = shelf.shelf_id,
            cartridges = shelf.cartridges,
            "cartridge shelf deferred for a day"
        );
        let ev = DomainEvent::StaffAlert {
            kind: StaffAlertKind::CartridgeShelfDeferred,
            detail: deferred_detail(&shelf),
        };
        if let Err(e) = bus.publish(Topic::Platform, ev).await {
            tracing::warn!(error = %e, "could not publish a cartridge shelf alert");
        }
    }
    Ok(out)
}

/// Register the re-check on `runner`. Pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: PgPool,
    arcade: ArcadeService,
    objects: Arc<dyn CartridgeObjects>,
    bus: Arc<dyn EventBus>,
    dm: DmSlot,
) -> JobRunner {
    runner.register(RECHECK_KIND, 1, move |job: Job| {
        let (pool, arcade, objects, bus, dm) = (
            pool.clone(),
            arcade.clone(),
            objects.clone(),
            bus.clone(),
            dm.clone(),
        );
        Box::pin(async move {
            let started = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
            let out = run(&arcade, &*objects, &bus, dm.get().map(|d| &**d), started).await?;
            if out.rewritten + out.delisted + out.deferred > 0 {
                tracing::info!(
                    rewritten = out.rewritten,
                    delisted = out.delisted,
                    deferred = out.deferred,
                    "cartridge manifests re-checked"
                );
            }
            // D-582: while any cartridge is deferred, look again in an hour.
            if arcade.deferred_cartridges().await? > 0 {
                jobs::reschedule_running(&pool, job.id, DEFER_RETRY).await?;
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
    use sr_activities::arcade::CartridgeReadError;
    use sr_db::accounts::{self, NewAccount};
    use sr_db::testing::TestDb;
    use sr_events::InProcessBus;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    static SEQ: AtomicI64 = AtomicI64::new(1);

    fn uid() -> i64 {
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0);
        base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
    }

    /// Every shelf answers the same way.
    struct Shelves(Result<Vec<u8>, CartridgeReadError>);

    #[async_trait::async_trait]
    impl CartridgeObjects for Shelves {
        async fn read_range(
            &self,
            _shelf: i64,
            _path: &str,
            start: u64,
            end: u64,
        ) -> Result<Vec<u8>, CartridgeReadError> {
            self.0.clone().map(|v| {
                let a = usize::try_from(start).unwrap_or(usize::MAX).min(v.len());
                let b = usize::try_from(end).unwrap_or(usize::MAX).min(v.len());
                v[a..b].to_vec()
            })
        }
    }

    fn not_found() -> Shelves {
        Shelves(Err(CartridgeReadError::Definite(
            "is not on its shelf (the object was not found).".to_owned(),
        )))
    }

    fn backend_down() -> Shelves {
        Shelves(Err(CartridgeReadError::Transient(
            "could not be read from its shelf. Try again later.".to_owned(),
        )))
    }

    /// Records the system DMs.
    #[derive(Default)]
    struct Dms(Mutex<Vec<(i64, String)>>);

    #[async_trait::async_trait]
    impl SystemDm for Dms {
        async fn dm(&self, to: i64, card: SystemCard) -> bool {
            self.0.lock().unwrap().push((to, card.content));
            true
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
            aged: false,
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

    /// D-593: an aged delisting tells the lister (and staff) to wait for the shelf, not to fix the file.
    #[test]
    fn an_aged_delisting_says_list_it_again_once_the_shelf_answers() {
        let c = DelistedCartridge {
            id: 7,
            guild_id: 9,
            guild_name: "Night Owls".into(),
            title: "Pong".into(),
            source_ref: "games/pong.zip".into(),
            added_by: 3,
            added_by_name: "ada".into(),
            reason: "its shelf has not answered for 7 days (last answer: its file could not be read from its shelf)."
                .into(),
            aged: true,
        };
        let dm = lister_dm_text(&c);
        assert!(dm.ends_with("(last answer: its file could not be read from its shelf). List it again once the shelf is answering again."), "{dm}");
        let alert = alert_detail(&c);
        assert!(
            alert.ends_with("The lister can list it again once the shelf is answering again."),
            "{alert}"
        );
        assert!(!alert.contains("sideroom.json"), "{alert}");
    }

    #[test]
    fn the_lister_dm_names_the_cartridge_the_server_the_reason_and_what_to_do() {
        let t = lister_dm_text(&DelistedCartridge {
            id: 7,
            guild_id: 9,
            guild_name: "Night Owls".into(),
            title: "Pong".into(),
            source_ref: "games/pong.zip".into(),
            added_by: 3,
            added_by_name: "ada".into(),
            reason: "has no sideroom.json.".into(),
            aged: false,
        });
        for part in [
            "\"Pong\"",
            "\"Night Owls\"",
            "has no sideroom.json.",
            "List it again once the file is fixed",
        ] {
            assert!(t.contains(part), "{t}");
        }
    }

    struct World {
        db: TestDb,
        arcade: ArcadeService,
        user: i64,
        name: String,
        guild: i64,
        shelf: i64,
    }

    /// A lister, a server named Night Owls and a shelf (storage mount) it owns.
    async fn world() -> World {
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
        let shelf = uid();
        sqlx::query(
            "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, \
                 access_key_id, secret_key_enc) \
             VALUES ($1, 'guild', $2, 'm', 'garage', 'http://127.0.0.1:3900', 'b', 'AK', $3)",
        )
        .bind(shelf)
        .bind(guild)
        .bind(vec![0u8])
        .execute(pool)
        .await
        .unwrap();
        let arcade = ArcadeService::new(pool.clone(), Arc::new(uid));
        World {
            db,
            arcade,
            user,
            name,
            guild,
            shelf,
        }
    }

    impl World {
        /// A cartridge row naming a file on the shelf; returns its id.
        async fn cartridge(&self, title: &str, path: &str) -> i64 {
            let pool = self.db.pool();
            let hash: String = sqlx::query_scalar("SELECT shelf_object_hash($1)")
                .bind(path)
                .fetch_one(pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO shelf_objects (shelf_id, object_hash, path, kind, size) \
                 VALUES ($1, $2, $3, 'games', 1000)",
            )
            .bind(self.shelf)
            .bind(&hash)
            .bind(path)
            .execute(pool)
            .await
            .unwrap();
            let id = uid();
            sqlx::query(
                "INSERT INTO arcade_guild_cartridges (id, guild_id, title, kind, tier, source_ref, added_by, \
                     shelf_id, object_hash) \
                 VALUES ($1, $2, $3, 'game', 'TIER_1_ORIGINAL', $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(self.guild)
            .bind(title)
            .bind(path)
            .bind(self.user)
            .bind(self.shelf)
            .bind(&hash)
            .execute(pool)
            .await
            .unwrap();
            id
        }
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn an_unreadable_old_cartridge_is_delisted_with_one_staff_alert() {
        let w = world().await;
        let pool = w.db.pool();
        sqlx::query(
            "INSERT INTO arcade_guild_cartridges (id, guild_id, title, kind, tier, source_ref, added_by) \
             VALUES ($1, $2, 'Lost Game', 'game', 'TIER_1_ORIGINAL', 'games/lost.zip', $3)",
        )
        .bind(uid())
        .bind(w.guild)
        .bind(w.user)
        .execute(pool)
        .await
        .unwrap();
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let mut alerts = bus.subscribe(Topic::Platform);

        let out = run(&w.arcade, &backend_down(), &bus, None, 1)
            .await
            .unwrap();
        // No shelf holds the path: a definite answer, whatever the shelves would say.
        assert_eq!((out.rewritten, out.delisted, out.deferred), (0, 1, 0));
        let ev = tokio::time::timeout(Duration::from_secs(2), alerts.next())
            .await
            .unwrap()
            .unwrap();
        let DomainEvent::StaffAlert { kind, detail } = ev else {
            panic!("expected a staff alert");
        };
        assert_eq!(kind, StaffAlertKind::CartridgeDelisted);
        assert!(
            detail.contains("\"Lost Game\"") && detail.contains(&format!("@{}", w.name)),
            "{detail}"
        );
        assert!(w.arcade.cartridges(w.guild).await.unwrap().is_empty());
        // Done once: a second run (the next boot) checks nothing and alerts nobody.
        let out = run(&w.arcade, &backend_down(), &bus, None, 2)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (0, 0));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), alerts.next())
                .await
                .is_err()
        );
        w.db.cleanup().await.unwrap();
    }

    /// D-582: an object the shelf says is not there is delisted; the lister gets one DM and the server's audit log
    /// an `arcade.cartridge_delisted` entry.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_missing_object_is_delisted_with_a_dm_and_an_audit_row() {
        let w = world().await;
        let pool = w.db.pool();
        let cartridge = w.cartridge("Gone Game", "games/gone.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let dm = Dms::default();

        let out = run(&w.arcade, &not_found(), &bus, Some(&dm), 1)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (1, 0));
        assert!(w.arcade.cartridges(w.guild).await.unwrap().is_empty());
        let (delisted, reason): (bool, Option<String>) = sqlx::query_as(
            "SELECT delisted_at IS NOT NULL, delist_reason FROM arcade_guild_cartridges WHERE id = $1",
        )
        .bind(cartridge)
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(delisted);
        assert!(reason.unwrap().contains("not found"));

        let dms = dm.0.lock().unwrap().clone();
        assert_eq!(dms.len(), 1, "{dms:?}");
        assert_eq!(dms[0].0, w.user);
        for part in [
            "\"Gone Game\"",
            "\"Night Owls\"",
            "not found",
            "List it again once the file is fixed",
        ] {
            assert!(dms[0].1.contains(part), "{}", dms[0].1);
        }

        let rows: Vec<(i16, Option<i64>, serde_json::Value, Option<String>)> = sqlx::query_as(
            "SELECT action, target_id, options, reason FROM audit_log_entries WHERE guild_id = $1",
        )
        .bind(w.guild)
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, 230);
        assert_eq!(rows[0].1, Some(cartridge));
        assert_eq!(rows[0].2["event"], "arcade.cartridge_delisted");
        assert_eq!(rows[0].2["title"], "Gone Game");
        assert!(rows[0].3.as_deref().unwrap().contains("not found"));

        // Once: the next run neither DMs nor writes another entry.
        run(&w.arcade, &not_found(), &bus, Some(&dm), 2)
            .await
            .unwrap();
        assert_eq!(dm.0.lock().unwrap().len(), 1);
        let n: i64 =
            sqlx::query_scalar("SELECT count(*) FROM audit_log_entries WHERE guild_id = $1")
                .bind(w.guild)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(n, 1);
        w.db.cleanup().await.unwrap();
    }

    /// D-582: a backend error is not an answer about the file: the row stays unchecked and listed, nobody is told,
    /// and the next run (a new `run`) looks again.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_backend_error_defers_the_cartridge_and_delists_nothing() {
        let w = world().await;
        let pool = w.db.pool();
        let cartridge = w.cartridge("Shy Game", "games/shy.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let mut alerts = bus.subscribe(Topic::Platform);
        let dm = Dms::default();

        let out = run(&w.arcade, &backend_down(), &bus, Some(&dm), 1)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred, out.shelves_alerted), (0, 1, 0));
        let (checked, delisted): (bool, bool) = sqlx::query_as(
            "SELECT manifest_checked_at IS NOT NULL, delisted_at IS NOT NULL \
             FROM arcade_guild_cartridges WHERE id = $1",
        )
        .bind(cartridge)
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(!checked && !delisted);
        assert_eq!(w.arcade.cartridges(w.guild).await.unwrap().len(), 1);
        assert_eq!(w.arcade.deferred_cartridges().await.unwrap(), 1);
        assert!(dm.0.lock().unwrap().is_empty());
        let n: i64 =
            sqlx::query_scalar("SELECT count(*) FROM audit_log_entries WHERE guild_id = $1")
                .bind(w.guild)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(n, 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), alerts.next())
                .await
                .is_err()
        );

        // The shelf answers on the next run: now it is a definite answer.
        let out = run(&w.arcade, &not_found(), &bus, Some(&dm), 2)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (1, 0));
        assert_eq!(w.arcade.deferred_cartridges().await.unwrap(), 0);
        w.db.cleanup().await.unwrap();
    }

    /// D-582: a shelf deferred for 24 hours raises exactly one staff alert, however many cartridges wait on it and
    /// however often the job runs again.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_shelf_deferred_for_a_day_raises_exactly_one_alert() {
        let w = world().await;
        let pool = w.db.pool();
        w.cartridge("One", "games/one.zip").await;
        w.cartridge("Two", "games/two.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let mut alerts = bus.subscribe(Topic::Platform);

        // Day 0: deferred, no alert yet.
        let out = run(&w.arcade, &backend_down(), &bus, None, 1)
            .await
            .unwrap();
        assert_eq!((out.deferred, out.shelves_alerted), (2, 0));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), alerts.next())
                .await
                .is_err()
        );

        // A day later the shelf still does not answer.
        sqlx::query(
            "UPDATE arcade_guild_cartridges SET manifest_deferred_since = now() - interval '25 hours' \
             WHERE guild_id = $1",
        )
        .bind(w.guild)
        .execute(pool)
        .await
        .unwrap();
        let out = run(&w.arcade, &backend_down(), &bus, None, 2)
            .await
            .unwrap();
        assert_eq!((out.deferred, out.delisted, out.shelves_alerted), (2, 0, 1));
        let ev = tokio::time::timeout(Duration::from_secs(2), alerts.next())
            .await
            .unwrap()
            .unwrap();
        let DomainEvent::StaffAlert { kind, detail } = ev else {
            panic!("expected a staff alert");
        };
        assert_eq!(kind, StaffAlertKind::CartridgeShelfDeferred);
        assert!(
            detail.contains(&format!("Shelf {}", w.shelf))
                && detail.contains("Night Owls")
                && detail.contains("2 server cartridge"),
            "{detail}"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(200), alerts.next())
                .await
                .is_err(),
            "one alert per shelf, not one per cartridge"
        );

        // The next hourly run raises nothing more, and nothing was delisted.
        let out = run(&w.arcade, &backend_down(), &bus, None, 3)
            .await
            .unwrap();
        assert_eq!((out.deferred, out.shelves_alerted), (2, 0));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), alerts.next())
                .await
                .is_err()
        );
        assert_eq!(w.arcade.cartridges(w.guild).await.unwrap().len(), 2);
        w.db.cleanup().await.unwrap();
    }

    /// Backdate every deferral of the server's cartridges to `interval` ago.
    async fn deferred_ago(w: &World, interval: &str) {
        sqlx::query(
            "UPDATE arcade_guild_cartridges SET manifest_deferred_since = now() - $2::text::interval \
             WHERE guild_id = $1 AND manifest_deferred_since IS NOT NULL",
        )
        .bind(w.guild)
        .bind(interval)
        .execute(w.db.pool())
        .await
        .unwrap();
    }

    /// D-590: six days and 23 hours of silence is not yet seven days: the cartridge stays listed.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_cartridge_deferred_for_six_days_and_23_hours_stays_listed() {
        let w = world().await;
        w.cartridge("Patient Game", "games/patient.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let dm = Dms::default();
        run(&w.arcade, &backend_down(), &bus, Some(&dm), 1)
            .await
            .unwrap();
        deferred_ago(&w, "6 days 23 hours").await;
        let out = run(&w.arcade, &backend_down(), &bus, Some(&dm), 2)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (0, 1));
        assert_eq!(w.arcade.cartridges(w.guild).await.unwrap().len(), 1);
        assert!(dm.0.lock().unwrap().is_empty());
        w.db.cleanup().await.unwrap();
    }

    /// D-590: at seven days the next transient answer delists, as a definite one: lister DM, audit entry (action
    /// 230, the whole reason), staff alert; and nothing is repeated afterwards.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_cartridge_deferred_for_seven_days_is_delisted_with_dm_audit_and_alert() {
        let w = world().await;
        let pool = w.db.pool();
        let cartridge = w.cartridge("Silent Game", "games/silent.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let mut alerts = bus.subscribe(Topic::Platform);
        let dm = Dms::default();
        run(&w.arcade, &backend_down(), &bus, Some(&dm), 1)
            .await
            .unwrap();
        deferred_ago(&w, "7 days").await;

        let out = run(&w.arcade, &backend_down(), &bus, Some(&dm), 2)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (1, 0));
        assert!(w.arcade.cartridges(w.guild).await.unwrap().is_empty());
        assert_eq!(w.arcade.deferred_cartridges().await.unwrap(), 0);
        let reason: Option<String> =
            sqlx::query_scalar("SELECT delist_reason FROM arcade_guild_cartridges WHERE id = $1")
                .bind(cartridge)
                .fetch_one(pool)
                .await
                .unwrap();
        let reason = reason.unwrap();
        assert!(
            reason.contains("its shelf has not answered for 7 days")
                && reason.contains("(last answer: its file could not be read from its shelf).")
                && !reason.contains("Try again later"),
            "{reason}"
        );

        let dms = dm.0.lock().unwrap().clone();
        assert_eq!(dms.len(), 1, "{dms:?}");
        assert_eq!(dms[0].0, w.user);
        for part in [
            "\"Silent Game\"",
            "\"Night Owls\"",
            "its shelf has not answered for 7 days (last answer: its file could not be read from its shelf).",
            "List it again once the shelf is answering again.",
        ] {
            assert!(dms[0].1.contains(part), "{}", dms[0].1);
        }

        let rows: Vec<(i16, Option<i64>, Option<String>)> = sqlx::query_as(
            "SELECT action, target_id, reason FROM audit_log_entries WHERE guild_id = $1",
        )
        .bind(w.guild)
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!((rows[0].0, rows[0].1), (230, Some(cartridge)));
        assert!(
            rows[0]
                .2
                .as_deref()
                .unwrap()
                .starts_with("its shelf has not answered for 7 days")
        );

        let ev = tokio::time::timeout(Duration::from_secs(2), alerts.next())
            .await
            .unwrap()
            .unwrap();
        let DomainEvent::StaffAlert { kind, detail } = ev else {
            panic!("expected a staff alert");
        };
        assert_eq!(kind, StaffAlertKind::CartridgeDelisted);
        assert!(
            detail.contains("\"Silent Game\"")
                && detail.contains("its shelf has not answered for 7 days")
                && detail.contains("games/silent.zip")
                && detail
                    .ends_with("The lister can list it again once the shelf is answering again."),
            "{detail}"
        );

        // Once: the next run does nothing more.
        run(&w.arcade, &backend_down(), &bus, Some(&dm), 3)
            .await
            .unwrap();
        assert_eq!(dm.0.lock().unwrap().len(), 1);
        w.db.cleanup().await.unwrap();
    }

    /// D-590: an answer in between ends the deferral for good: a readable file is checked and listed, however old
    /// its first deferral was.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn an_answer_in_between_resets_the_clock() {
        use std::io::Write;
        let w = world().await;
        let pool = w.db.pool();
        let cartridge = w.cartridge("Back Game", "games/back.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let dm = Dms::default();
        run(&w.arcade, &backend_down(), &bus, Some(&dm), 1)
            .await
            .unwrap();
        deferred_ago(&w, "6 days").await;

        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.start_file("sideroom.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(br#"{"tier":"TIER_1_ORIGINAL"}"#).unwrap();
        let bytes = z.finish().unwrap().into_inner();
        let out = run(&w.arcade, &Shelves(Ok(bytes)), &bus, Some(&dm), 2)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (0, 0));
        assert_eq!(w.arcade.deferred_cartridges().await.unwrap(), 0);

        // Days later the shelf is down again, but this cartridge is settled: nothing is looked at or delisted.
        deferred_ago(&w, "30 days").await;
        let out = run(&w.arcade, &backend_down(), &bus, Some(&dm), 3)
            .await
            .unwrap();
        assert_eq!((out.delisted, out.deferred), (0, 0));
        let delisted: bool = sqlx::query_scalar(
            "SELECT delisted_at IS NOT NULL FROM arcade_guild_cartridges WHERE id = $1",
        )
        .bind(cartridge)
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(!delisted);
        assert!(dm.0.lock().unwrap().is_empty());
        w.db.cleanup().await.unwrap();
    }

    /// D-590: the 24-hour alert names the day the cartridges will be delisted (seven days after the first
    /// deferral, UTC).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn the_24_hour_alert_names_the_delist_day() {
        let w = world().await;
        w.cartridge("One", "games/one.zip").await;
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let mut alerts = bus.subscribe(Topic::Platform);
        run(&w.arcade, &backend_down(), &bus, None, 1)
            .await
            .unwrap();
        deferred_ago(&w, "25 hours").await;
        let day: String = sqlx::query_scalar(
            "SELECT to_char((manifest_deferred_since AT TIME ZONE 'UTC') + interval '7 days', 'YYYY-MM-DD') \
             FROM arcade_guild_cartridges WHERE guild_id = $1",
        )
        .bind(w.guild)
        .fetch_one(w.db.pool())
        .await
        .unwrap();
        let out = run(&w.arcade, &backend_down(), &bus, None, 2)
            .await
            .unwrap();
        assert_eq!((out.shelves_alerted, out.delisted), (1, 0));
        let ev = tokio::time::timeout(Duration::from_secs(2), alerts.next())
            .await
            .unwrap()
            .unwrap();
        let DomainEvent::StaffAlert { detail, .. } = ev else {
            panic!("expected a staff alert");
        };
        assert!(
            detail.contains(&format!("delisted on {day} UTC")) && detail.contains("7 days"),
            "{detail}"
        );
        w.db.cleanup().await.unwrap();
    }
}
