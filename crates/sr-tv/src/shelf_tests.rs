//! BYOS-TV-001: Live TV over server shelves. A server channel draws from the server's shelves only, one guide
//! for every viewer; a personal mount is refused on it with the reason; an item that cannot air is left out
//! for everyone alike; a channel is visible only to those who may watch it.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use sr_core::{ManualClock, PlatformPermissions as P};

use crate::access::ChannelAccess;
use crate::catalog::{CatalogItem, MemoryCatalog, item};
use crate::error::{TvError, TvResult};
use crate::model::*;
use crate::repo::MemoryTvRepo;
use crate::service::{Actor, PlaceInput, TvService, Viewer};
use crate::tz::{MS_DAY, days_from_civil};

const GUILD: PoolSource = PoolSource::Guild { guild_id: 900 };
const MINE: PoolSource = PoolSource::Personal { user_id: 5 };

/// The instance bucket is mount 1, the server's own bucket 2, a member's personal bucket 3.
fn on(mount_id: i64, title_id: i64, name: &str) -> CatalogItem {
    CatalogItem {
        mount_id,
        ..item(title_id, name, 30)
    }
}

struct Members(Vec<i64>);

#[async_trait]
impl ChannelAccess for Members {
    async fn may_watch(&self, viewer: Viewer, _pool: PoolSource) -> TvResult<bool> {
        Ok(self.0.contains(&viewer.user_id))
    }
}

struct Env {
    svc: TvService,
    catalog: MemoryCatalog,
    staff: Actor,
}

fn viewer(id: i64) -> Viewer {
    Viewer {
        user_id: id,
        adult: true,
    }
}

fn env() -> Env {
    let now = days_from_civil(2025, 6, 2) * MS_DAY + 20 * 3_600_000;
    let clock = Arc::new(ManualClock::new(now));
    let catalog = MemoryCatalog::new();
    catalog
        .add(on(1, 11, "Instance Film"))
        .add(on(2, 22, "Server Film"))
        .add(on(3, 33, "Personal Film"));
    catalog.allow_mounts(GUILD, vec![1, 2]);
    catalog.allow_mounts(MINE, vec![3]);
    let next = Arc::new(AtomicI64::new(1000));
    let svc = TvService::new(
        Arc::new(MemoryTvRepo::new()),
        Arc::new(catalog.clone()),
        clock,
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
    .with_access(Arc::new(Members(vec![5, 6])));
    Env {
        svc,
        catalog,
        staff: Actor {
            user_id: 1,
            permissions: P::MANAGE_TV,
        },
    }
}

fn all_day() -> ChannelRules {
    ChannelRules {
        grid_minutes: 30,
        no_repeat_window: 0,
        dayparts: vec![Daypart {
            name: "All day".into(),
            start: "00:00".into(),
            end: "24:00".into(),
            days: vec![],
            mode: PickMode::Sequential,
            fit: Fit::Overrun,
            pool: PoolFilter::default(),
        }],
    }
}

fn input(number: i32, pool: PoolSource, kind: ChannelKind) -> ChannelInput {
    ChannelInput {
        number,
        name: format!("Channel {number}"),
        call_sign: None,
        era_label: None,
        theme: None,
        logo_key: None,
        enabled: true,
        kind,
        timezone: "UTC".into(),
        age_restricted: false,
        filler_policy: FillerPolicy::Slate,
        rules: all_day(),
        pool,
    }
}

async fn make(e: &Env, number: i32, pool: PoolSource, kind: ChannelKind) -> i64 {
    e.svc
        .create_channel(e.staff, input(number, pool, kind))
        .await
        .unwrap()
        .id
}

async fn aired(e: &Env, channel: i64) -> Vec<String> {
    let g = e.svc.guide(None, None, &[channel]).await.unwrap();
    let mut names: Vec<String> = g.channels[0]
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Program)
        .map(|b| b.title.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[tokio::test]
async fn a_server_channel_airs_only_the_servers_shelves_and_the_attached_instance_bucket() {
    let e = env();
    let id = make(&e, 1, GUILD, ChannelKind::RuleBased).await;
    assert_eq!(aired(&e, id).await, ["Instance Film", "Server Film"]);
}

#[tokio::test]
async fn a_personal_channel_airs_only_its_owners_shelf() {
    let e = env();
    let id = make(&e, 1, MINE, ChannelKind::RuleBased).await;
    assert_eq!(aired(&e, id).await, ["Personal Film"]);
}

#[tokio::test]
async fn an_instance_channel_is_not_limited_by_mount() {
    let e = env();
    let id = make(&e, 1, PoolSource::Instance, ChannelKind::RuleBased).await;
    assert_eq!(
        aired(&e, id).await,
        ["Instance Film", "Personal Film", "Server Film"]
    );
}

#[tokio::test]
async fn a_server_without_the_instance_bucket_attached_airs_only_its_own_mounts() {
    let e = env();
    e.catalog.allow_mounts(GUILD, vec![2]);
    let id = make(&e, 1, GUILD, ChannelKind::RuleBased).await;
    assert_eq!(aired(&e, id).await, ["Server Film"]);
}

#[tokio::test]
async fn items_that_cannot_air_are_left_out_for_everyone() {
    let e = env();
    e.catalog.clear();
    e.catalog.add(on(2, 22, "Server Film")).add(CatalogItem {
        playable: false,
        ..on(2, 23, "Needs Conversion")
    });
    let id = make(&e, 1, GUILD, ChannelKind::RuleBased).await;
    assert_eq!(aired(&e, id).await, ["Server Film"]);
}

#[tokio::test]
async fn the_editor_refuses_a_personal_mount_on_a_server_channel_with_the_reason() {
    let e = env();
    let id = make(&e, 1, GUILD, ChannelKind::Manual).await;
    let place = |title_id| PlaceInput {
        title_id,
        episode_id: None,
        start_ms: days_from_civil(2025, 6, 2) * MS_DAY + 21 * 3_600_000,
        end_ms: None,
    };
    // The server's own film and the attached instance bucket's film are fine.
    e.svc.place_block(e.staff, id, place(22)).await.unwrap();
    e.svc.place_block(e.staff, id, place(11)).await.unwrap();
    // A member's personal film is not.
    let err = e.svc.place_block(e.staff, id, place(33)).await.unwrap_err();
    match err {
        TvError::Invalid(m) => assert!(m.contains("never a member's personal shelf"), "{m}"),
        other => panic!("{other:?}"),
    }
    // An item that cannot air is refused as unplayable, not as off-shelf.
    e.catalog.add(CatalogItem {
        playable: false,
        ..on(2, 24, "Needs Conversion")
    });
    assert!(matches!(
        e.svc.place_block(e.staff, id, place(24)).await,
        Err(TvError::Invalid(m)) if m.contains("cannot be played")
    ));
}

#[tokio::test]
async fn only_those_who_may_watch_see_a_server_channel_and_a_personal_one_is_private() {
    let e = env();
    let open = make(&e, 1, PoolSource::Instance, ChannelKind::RuleBased).await;
    let club = make(&e, 2, GUILD, ChannelKind::RuleBased).await;
    let mine = make(&e, 3, MINE, ChannelKind::RuleBased).await;

    let ids = |v: Vec<Channel>| v.into_iter().map(|c| c.id).collect::<Vec<_>>();
    let all = e.svc.channels().await.unwrap();
    // A member of the server who holds the grant (5, who also owns the personal channel).
    assert_eq!(
        ids(e.svc.visible_channels(viewer(5), all.clone()).await.unwrap()),
        [open, club, mine]
    );
    // Another member of the server: the server channel, not someone else's personal one.
    assert_eq!(
        ids(e.svc.visible_channels(viewer(6), all.clone()).await.unwrap()),
        [open, club]
    );
    // A stranger sees only the instance channel.
    assert_eq!(
        ids(e.svc.visible_channels(viewer(7), all).await.unwrap()),
        [open]
    );

    let g = e.svc.guide(None, None, &[]).await.unwrap();
    let seen = e.svc.visible_guide(viewer(7), g).await.unwrap();
    assert_eq!(seen.channels.len(), 1);

    // Tuning is refused as "no such channel".
    assert!(e.svc.tune(viewer(5), club).await.is_ok());
    assert!(matches!(
        e.svc.tune(viewer(7), club).await,
        Err(TvError::NotFound(_))
    ));
    assert!(matches!(
        e.svc.tune(viewer(6), mine).await,
        Err(TvError::NotFound(_))
    ));
    assert!(matches!(
        e.svc.require_watch(viewer(7), club).await,
        Err(TvError::NotFound(_))
    ));
}

#[tokio::test]
async fn without_an_access_port_a_server_channel_is_watched_by_nobody() {
    let e = env();
    let bare = TvService::new(
        Arc::new(MemoryTvRepo::new()),
        Arc::new(e.catalog.clone()),
        Arc::new(ManualClock::new(0)),
        Arc::new(|| 1),
    );
    assert!(!bare.may_watch(viewer(5), GUILD).await.unwrap());
    assert!(bare.may_watch(viewer(5), PoolSource::Instance).await.unwrap());
    assert!(bare.may_watch(viewer(5), MINE).await.unwrap());
    assert!(!bare.may_watch(viewer(6), MINE).await.unwrap());
}

#[tokio::test]
async fn channel_numbers_are_unique_within_a_source() {
    let e = env();
    make(&e, 1, GUILD, ChannelKind::RuleBased).await;
    make(&e, 1, PoolSource::Guild { guild_id: 901 }, ChannelKind::RuleBased).await;
    make(&e, 1, PoolSource::Instance, ChannelKind::RuleBased).await;
    let dup = e
        .svc
        .create_channel(e.staff, input(1, GUILD, ChannelKind::RuleBased))
        .await;
    assert!(matches!(dup, Err(TvError::Conflict(_))));
}
