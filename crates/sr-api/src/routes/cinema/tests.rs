//! Endpoint tests of the Cinema router over a stub repository: authentication and staff
//! matrices, permission mapping, the age gate, parameter handling, the staff access log and the
//! JSON shape. Behaviour against real tables is covered by `sr-db/tests/cinema_repo.rs`.

use super::{CinemaDeps, CinemaPrefs, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::{admin::AdminDeps, search::AgeGate},
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_cinema::error::{CinemaError, CinemaResult};
use sr_cinema::model::*;
use sr_cinema::repo::{AssetOwner, AssetUse, CinemaRepository, EpisodeQuery, NewTitle, Rows};
use sr_cinema::rows::{ProvidedRow, RowContext, RowEntry, RowProvider};
use sr_cinema::{CinemaService, TitleIndex};
use sr_media::playback::{MemoryPlaybackSource, sample_asset};

use super::PlaybackDeps;
use sr_core::{PlatformPermissions as P, SystemClock, UserId};
use sr_storage::cdn::CdnSigner;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

const OWNER: i64 = 10;
const CURATOR: i64 = 11;
const BASIC: i64 = 12;
pub(super) const MEMBER: i64 = 20;
pub(super) const ADULT: i64 = 21;
/// Confirmed adult who set `cinema.hide_age_restricted`.
const HIDER: i64 = 22;
const R_OWNER: i64 = 1;
const R_CURATOR: i64 = 2;
const R_BASIC: i64 = 3;

/// Title 7 is plain, title 9 is age-restricted; both are published films with an asset.
fn record(id: i64) -> TitleRecord {
    TitleRecord {
        id,
        kind: TitleKind::Movie,
        title: "Stub".into(),
        original_title: None,
        alt_titles: vec![],
        sort_title: "stub".into(),
        year: Some(1999),
        runtime_min: Some(100),
        content_rating: None,
        synopsis: String::new(),
        tagline: None,
        genres: vec![],
        tags: vec![],
        languages: vec![],
        subtitle_languages: vec![],
        cast: vec![],
        directors: vec![],
        writers: vec![],
        studios: vec![],
        crew: vec![],
        age_restricted: id == 9,
        poster_key: Some(format!("cinema/{id}/poster.webp")),
        backdrop_key: None,
        logo_key: None,
        external_ids: Default::default(),
        asset_id: Some(500 + id),
        published: true,
        published_at_ms: Some(1),
        merged_into: None,
        season_count: 0,
        episode_count: 0,
        last_content_at_ms: Some(1),
        has_trailer: false,
        created_at_ms: 1,
        updated_at_ms: 1,
        sort_value: Some(SortValue::Int(1)),
    }
}

/// What the stub saw of a browse request.
#[derive(Debug, Clone, PartialEq)]
struct Browsed {
    scope: Scope,
    age: AgeFilter,
    sort: SortKey,
    after: bool,
    collection: Option<i64>,
}

#[derive(Default)]
pub(crate) struct Stub {
    browses: Mutex<Vec<Browsed>>,
}

struct Confirmed(Mutex<HashSet<UserId>>);

#[async_trait]
impl AgeGate for Confirmed {
    async fn age_confirmed(&self, user: UserId) -> bool {
        self.0.lock().unwrap().contains(&user)
    }
}

struct Hiders(HashSet<UserId>);

#[async_trait]
impl CinemaPrefs for Hiders {
    async fn hide_age_restricted(&self, user: UserId) -> bool {
        self.0.contains(&user)
    }
}

fn unsupported<T>() -> CinemaResult<T> {
    Err(CinemaError::Storage("not stubbed".into()))
}

#[async_trait]
impl CinemaRepository for Stub {
    async fn browse(&self, q: &BrowseQuery, scope: Scope) -> CinemaResult<Rows<TitleRecord>> {
        self.browses.lock().unwrap().push(Browsed {
            scope,
            age: q.age,
            sort: q.sort,
            after: q.after.is_some(),
            collection: q.collection,
        });
        Ok(Rows {
            items: vec![record(7)],
            total: 1,
        })
    }
    async fn get_title(&self, id: i64, _scope: Scope) -> CinemaResult<Option<TitleRecord>> {
        Ok(matches!(id, 7 | 9).then(|| record(id)))
    }
    async fn titles_by_ids(
        &self,
        ids: &[i64],
        _scope: Scope,
        _include_restricted: bool,
    ) -> CinemaResult<Vec<TitleRecord>> {
        Ok(ids
            .iter()
            .filter(|i| matches!(**i, 7 | 9))
            .map(|i| record(*i))
            .collect())
    }
    async fn facets(&self, _: Option<TitleKind>, _: bool) -> CinemaResult<Facets> {
        Ok(Facets::default())
    }
    async fn list_seasons(&self, _: i64, _: Scope) -> CinemaResult<Vec<SeasonRecord>> {
        Ok(vec![])
    }
    async fn get_season(&self, _: i64) -> CinemaResult<Option<SeasonRecord>> {
        unsupported()
    }
    async fn list_episodes(
        &self,
        _: i64,
        _: &EpisodeQuery,
        _: Scope,
    ) -> CinemaResult<Vec<EpisodeRecord>> {
        Ok(vec![])
    }
    async fn get_episode(&self, _: i64) -> CinemaResult<Option<EpisodeRecord>> {
        Ok(None)
    }
    async fn episode_neighbours(&self, _: i64) -> CinemaResult<(Option<i64>, Option<i64>)> {
        unsupported()
    }
    async fn first_playable_episode(&self, _: i64) -> CinemaResult<Option<i64>> {
        Ok(None)
    }
    async fn list_variants(&self, _: i64) -> CinemaResult<Vec<VariantRecord>> {
        Ok(vec![])
    }
    async fn get_variant(&self, _: i64) -> CinemaResult<Option<VariantRecord>> {
        Ok(None)
    }
    async fn list_trailers(&self, _: i64) -> CinemaResult<Vec<TrailerRecord>> {
        Ok(vec![])
    }
    async fn title_contributors(&self, title: i64) -> CinemaResult<Vec<Contributor>> {
        Ok(if title == 7 {
            vec![Contributor {
                user_id: 42,
                name: "Mina".into(),
                titles: 1,
                episodes: 0,
            }]
        } else {
            vec![]
        })
    }
    async fn list_markers(&self, title: i64, _: Option<i64>) -> CinemaResult<Vec<MarkerRecord>> {
        Ok(if title == 7 {
            vec![MarkerRecord {
                id: 1,
                episode_id: None,
                kind: MarkerKind::Intro,
                start_ms: 1000,
                end_ms: 9000,
                source: "manual".into(),
            }]
        } else {
            vec![]
        })
    }
    /// Asset 507 is title 7's film, 509 the restricted title 9's, 777 a trailer of title 7 and 888 belongs to
    /// the unknown (unpublished) title 8.
    async fn asset_owner(&self, asset: i64) -> CinemaResult<Option<AssetOwner>> {
        let (title_id, usage) = match asset {
            507 => (7, AssetUse::Feature),
            509 => (9, AssetUse::Feature),
            777 => (7, AssetUse::Trailer(1)),
            888 => (8, AssetUse::Feature),
            _ => return Ok(None),
        };
        Ok(Some(AssetOwner {
            title_id,
            episode_id: None,
            usage,
        }))
    }
    async fn list_collections(
        &self,
        _: Option<Surface>,
        _: Scope,
    ) -> CinemaResult<Vec<CollectionRecord>> {
        Ok(vec![])
    }
    async fn get_collection(&self, _: i64, _: Scope) -> CinemaResult<Option<CollectionRecord>> {
        Ok(None)
    }
    async fn index_batch(&self, _: i64, _: u32) -> CinemaResult<Vec<TitleRecord>> {
        Ok(vec![])
    }
    async fn create_title(&self, t: &NewTitle) -> CinemaResult<TitleRecord> {
        let mut r = record(t.id);
        r.title = t.input.title.clone();
        r.genres = t.input.genres.clone();
        r.published = false;
        r.asset_id = None;
        r.poster_key = t.input.poster_key.clone();
        Ok(r)
    }
    async fn update_title(
        &self,
        id: i64,
        i: &TitleInput,
        _: &str,
    ) -> CinemaResult<Option<TitleRecord>> {
        let mut r = record(id);
        r.title = i.title.clone();
        Ok(matches!(id, 7 | 9).then_some(r))
    }
    async fn set_title_published(&self, id: i64, p: bool) -> CinemaResult<Option<TitleRecord>> {
        let mut r = record(id);
        r.published = p;
        Ok(matches!(id, 7 | 9).then_some(r))
    }
    async fn set_title_asset(&self, id: i64, a: Option<i64>) -> CinemaResult<Option<TitleRecord>> {
        let mut r = record(id);
        r.asset_id = a;
        Ok(matches!(id, 7 | 9).then_some(r))
    }
    async fn delete_title(&self, id: i64) -> CinemaResult<Option<Vec<i64>>> {
        Ok(matches!(id, 7 | 9).then(Vec::new))
    }
    async fn merge_titles(&self, source: i64, target: i64) -> CinemaResult<MergeOutcome> {
        if source == 7 && target == 9 {
            Ok(MergeOutcome {
                target: record(9),
                episode_map: vec![],
            })
        } else {
            Err(CinemaError::NotFound("title"))
        }
    }
    async fn create_season(&self, _: i64, _: i64, _: &SeasonInput) -> CinemaResult<SeasonRecord> {
        Err(CinemaError::Conflict("season 1 already exists".into()))
    }
    async fn update_season(&self, _: i64, _: &SeasonInput) -> CinemaResult<Option<SeasonRecord>> {
        Ok(None)
    }
    async fn delete_season(&self, _: i64) -> CinemaResult<Option<(i64, Vec<i64>)>> {
        Ok(None)
    }
    async fn create_episode(
        &self,
        _: i64,
        _: i64,
        _: &EpisodeInput,
    ) -> CinemaResult<EpisodeRecord> {
        Err(CinemaError::NotFound("season"))
    }
    async fn update_episode(
        &self,
        _: i64,
        _: &EpisodeInput,
    ) -> CinemaResult<Option<EpisodeRecord>> {
        Ok(None)
    }
    async fn set_episode_published(&self, _: i64, _: bool) -> CinemaResult<Option<EpisodeRecord>> {
        Ok(None)
    }
    async fn set_episode_asset(
        &self,
        _: i64,
        _: Option<i64>,
    ) -> CinemaResult<Option<EpisodeRecord>> {
        Ok(None)
    }
    async fn delete_episode(&self, _: i64) -> CinemaResult<Option<i64>> {
        Ok(None)
    }
    async fn create_variant(
        &self,
        _: i64,
        _: i64,
        _: &VariantInput,
    ) -> CinemaResult<VariantRecord> {
        unsupported()
    }
    async fn update_variant(
        &self,
        _: i64,
        _: &VariantInput,
    ) -> CinemaResult<Option<VariantRecord>> {
        Ok(None)
    }
    async fn delete_variant(&self, _: i64) -> CinemaResult<bool> {
        Ok(false)
    }
    async fn create_trailer(
        &self,
        _: i64,
        _: i64,
        _: &TrailerInput,
    ) -> CinemaResult<TrailerRecord> {
        unsupported()
    }
    async fn update_trailer(
        &self,
        _: i64,
        _: &TrailerInput,
    ) -> CinemaResult<Option<TrailerRecord>> {
        Ok(None)
    }
    async fn delete_trailer(&self, _: i64) -> CinemaResult<bool> {
        Ok(false)
    }
    async fn replace_markers(
        &self,
        _: i64,
        _: Option<i64>,
        markers: &[MarkerInput],
        ids: &[i64],
        _: i64,
    ) -> CinemaResult<Option<Vec<MarkerRecord>>> {
        Ok(Some(
            markers
                .iter()
                .zip(ids)
                .map(|(m, id)| MarkerRecord {
                    id: *id,
                    episode_id: None,
                    kind: m.kind,
                    start_ms: m.start_ms,
                    end_ms: m.end_ms,
                    source: "manual".into(),
                })
                .collect(),
        ))
    }
    async fn create_collection(
        &self,
        id: i64,
        i: &CollectionInput,
        _: i64,
    ) -> CinemaResult<CollectionRecord> {
        Ok(CollectionRecord {
            id,
            slug: i.slug.clone(),
            name: i.name.clone(),
            eyebrow: i.eyebrow.clone(),
            blurb: i.blurb.clone(),
            display: i.display,
            surfaces: i.surfaces.clone(),
            position: i.position,
            published: i.published,
            item_count: 0,
        })
    }
    async fn update_collection(
        &self,
        _: i64,
        _: &CollectionInput,
    ) -> CinemaResult<Option<CollectionRecord>> {
        Ok(None)
    }
    async fn delete_collection(&self, _: i64) -> CinemaResult<bool> {
        Ok(true)
    }
    async fn set_collection_items(
        &self,
        _: i64,
        _: &[i64],
    ) -> CinemaResult<Option<CollectionRecord>> {
        Ok(None)
    }
}

type Asked = Arc<Mutex<Vec<(i64, u32, HashMap<String, String>)>>>;

/// A provider that records what it was asked.
struct Seen {
    asked: Asked,
}

#[async_trait]
impl RowProvider for Seen {
    fn key(&self) -> &'static str {
        "trending"
    }
    async fn row(&self, ctx: &RowContext) -> CinemaResult<Option<ProvidedRow>> {
        self.asked.lock().unwrap().push((
            ctx.viewer.user_id,
            ctx.limit,
            ctx.params.clone().into_iter().collect(),
        ));
        Ok(Some(ProvidedRow {
            entries: vec![RowEntry::title(7), RowEntry::title(404)],
            ..ProvidedRow::default()
        }))
    }
}

pub(super) struct Env {
    pub(super) app: Router,
    pub(super) source: MemoryPlaybackSource,
    pub(super) playback: Arc<PlaybackDeps>,
    stub: Arc<Stub>,
    admin: Arc<MemoryAdminRepo>,
    age: Arc<Confirmed>,
    pub(super) tokens: HashMap<i64, String>,
    asked: Asked,
}

pub(super) const SIGNER_KEY: [u8; 32] = [3u8; 32];

async fn env_with(cinema: bool) -> Env {
    let admin = Arc::new(MemoryAdminRepo::default());
    admin.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    admin.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name) in [
        (CURATOR, "curator"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (ADULT, "adult"),
        (HIDER, "hider"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(R_OWNER, "Owner", 3, P::OWNER);
    admin.add_role(
        R_CURATOR,
        "Media Curator",
        2,
        P::STAFF_BASIC | P::PUBLISH_MEDIA | P::MANAGE_COLLECTIONS,
    );
    admin.add_role(R_BASIC, "Basic", 1, P::STAFF_BASIC);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, CURATOR, &[R_CURATOR], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[R_BASIC], Some("test"))
        .await
        .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, CURATOR, BASIC, MEMBER, ADULT, HIDER]
        .into_iter()
        .enumerate()
    {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(uid, t.expose().to_owned());
    }
    let bots = Arc::new(MemoryBots::new());
    bots.insert(
        "bot",
        BotIdentity {
            user_id: MEMBER,
            application_id: 1,
        },
    );

    let stub = Arc::new(Stub::default());
    let asked: Asked = Arc::new(Mutex::new(Vec::new()));
    let service = CinemaService::new(
        stub.clone(),
        CdnSigner::new(SIGNER_KEY.to_vec()),
        Arc::new(SystemClock),
        Arc::new(|| 4242),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()))
    .with_storage(Arc::new(sr_storage::ObjectStoreStorage::in_memory()))
    .with_provider(Arc::new(Seen {
        asked: asked.clone(),
    }));
    let age = Arc::new(Confirmed(Mutex::new(HashSet::from([
        UserId::new(ADULT as u64),
        UserId::new(HIDER as u64),
    ]))));
    let source = MemoryPlaybackSource::new();
    source.insert(sample_asset(507));
    source.insert(sample_asset(509));
    source.insert(sample_asset(777));
    source.insert(sample_asset(888));
    let playback = Arc::new(PlaybackDeps::new(
        Arc::new(source.clone()),
        Arc::new(CdnSigner::new(SIGNER_KEY.to_vec())),
        Arc::new(SystemClock),
    ));
    let deps = CinemaDeps::new(service)
        .with_playback(playback.clone())
        .with_age_gate(age.clone())
        .with_prefs(Arc::new(Hiders(HashSet::from([UserId::new(HIDER as u64)]))));

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let mut b = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .extension(Arc::new(AdminDeps::new(svc)));
    if cinema {
        b = b.extension(Arc::new(deps));
    }
    Env {
        app: build_router(b.build(), router()),
        source,
        playback,
        stub,
        admin,
        age,
        tokens,
        asked,
    }
}

pub(super) async fn env() -> Env {
    env_with(true).await
}

impl Env {
    pub(super) async fn call(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let raw = &self.tokens[&uid];
            req = req.header("cookie", format!("sr_session={raw}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(raw));
            }
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn browses(&self) -> Vec<Browsed> {
        self.stub.browses.lock().unwrap().clone()
    }
}

fn title_body() -> Value {
    json!({
        "kind": "movie", "title": "  New   Film ", "genres": ["Drama"],
        "poster_key": "cinema/4242/poster.webp", "external_ids": {"IMDB": "tt1"}
    })
}

type Route = (Method, &'static str, Option<Value>);

fn member_routes() -> Vec<Route> {
    vec![
        (Method::GET, "/cinema/discover", None),
        (Method::GET, "/cinema/rows/new-this-week", None),
        (Method::GET, "/cinema/collections", None),
        (Method::GET, "/cinema/titles", None),
        (Method::GET, "/cinema/facets", None),
        (Method::GET, "/cinema/titles/7", None),
        (Method::GET, "/cinema/titles/7/episodes", None),
        (Method::GET, "/cinema/titles/7/markers", None),
        (Method::GET, "/cinema/titles/7/contributors", None),
        (Method::GET, "/cinema/titles/7/play", None),
        (Method::GET, "/cinema/episodes/5", None),
        (Method::GET, "/cinema/episodes/5/markers", None),
        (Method::GET, "/cinema/episodes/5/play", None),
        (Method::GET, "/cinema/search?q=a", None),
        (Method::GET, "/cinema/assets/507/playback", None),
        (Method::POST, "/cinema/assets/507/refresh", None),
    ]
}

fn admin_routes() -> Vec<Route> {
    let t = Some(title_body());
    let season = Some(json!({ "number": 1 }));
    let episode = Some(json!({ "number": 1, "title": "Pilot" }));
    let variant = Some(json!({ "label": "Director's Cut" }));
    let trailer = Some(json!({ "label": "Trailer" }));
    let markers = Some(json!({ "markers": [{ "kind": "intro", "start_ms": 0, "end_ms": 10 }] }));
    let collection = Some(json!({ "slug": "picks", "name": "Picks" }));
    vec![
        (Method::GET, "/admin/cinema/titles", None),
        (Method::POST, "/admin/cinema/titles", t.clone()),
        (Method::GET, "/admin/cinema/titles/7", None),
        (Method::PUT, "/admin/cinema/titles/7", t),
        (Method::DELETE, "/admin/cinema/titles/7", None),
        (Method::POST, "/admin/cinema/titles/7/publish", None),
        (Method::POST, "/admin/cinema/titles/7/unpublish", None),
        (
            Method::POST,
            "/admin/cinema/titles/7/merge",
            Some(json!({ "target_id": "9" })),
        ),
        (
            Method::PUT,
            "/admin/cinema/titles/7/asset",
            Some(json!({ "asset_id": "1" })),
        ),
        (
            Method::POST,
            "/admin/cinema/titles/7/artwork",
            Some(json!({ "slot": "poster", "content_type": "image/webp" })),
        ),
        (
            Method::POST,
            "/admin/cinema/titles/7/seasons",
            season.clone(),
        ),
        (Method::PUT, "/admin/cinema/seasons/3", season),
        (Method::DELETE, "/admin/cinema/seasons/3", None),
        (
            Method::POST,
            "/admin/cinema/seasons/3/episodes",
            episode.clone(),
        ),
        (Method::PUT, "/admin/cinema/episodes/5", episode),
        (Method::DELETE, "/admin/cinema/episodes/5", None),
        (Method::POST, "/admin/cinema/episodes/5/publish", None),
        (Method::POST, "/admin/cinema/episodes/5/unpublish", None),
        (
            Method::PUT,
            "/admin/cinema/episodes/5/asset",
            Some(json!({ "asset_id": null })),
        ),
        (Method::GET, "/admin/cinema/episodes/5/markers", None),
        (
            Method::PUT,
            "/admin/cinema/episodes/5/markers",
            markers.clone(),
        ),
        (Method::PUT, "/admin/cinema/titles/7/markers", markers),
        (
            Method::POST,
            "/admin/cinema/titles/7/variants",
            variant.clone(),
        ),
        (Method::PUT, "/admin/cinema/variants/6", variant),
        (Method::DELETE, "/admin/cinema/variants/6", None),
        (
            Method::POST,
            "/admin/cinema/titles/7/trailers",
            trailer.clone(),
        ),
        (Method::PUT, "/admin/cinema/trailers/8", trailer),
        (Method::DELETE, "/admin/cinema/trailers/8", None),
        (Method::GET, "/admin/cinema/collections", None),
        (
            Method::POST,
            "/admin/cinema/collections",
            collection.clone(),
        ),
        (Method::GET, "/admin/cinema/collections/2", None),
        (Method::PUT, "/admin/cinema/collections/2", collection),
        (Method::DELETE, "/admin/cinema/collections/2", None),
        (
            Method::PUT,
            "/admin/cinema/collections/2/items",
            Some(json!({ "title_ids": ["7"] })),
        ),
    ]
}

#[tokio::test]
async fn a_title_lists_its_credited_contributors() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/cinema/titles/7/contributors",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["contributors"][0]["name"], "Mina");
    assert_eq!(body["contributors"][0]["titles"], 1);
    // An unknown or unpublished title answers 404, not an empty list.
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/cinema/titles/8/contributors",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn member_routes_need_a_signed_in_user() {
    let e = env().await;
    for (m, p, b) in member_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
    }
    // Bots are refused.
    let resp = e
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/cinema/titles")
                .header("authorization", "Bot bot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_routes_need_staff_with_the_right_permission() {
    let e = env().await;
    for (m, p, b) in admin_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "anonymous {m} {p}");
        // Not staff at all.
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "member {m} {p}");
        assert_eq!(body["code"], 900002, "{m} {p}");
        // Staff without cinema permissions.
        let (s, body) = e.call(Some(BASIC), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "basic {m} {p}");
        assert_eq!(body["code"], 50013, "{m} {p}");
    }
}

#[tokio::test]
async fn a_media_curator_publishes_but_only_the_owner_deletes_and_merges() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            "/admin/cinema/titles/7",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(body["message"].as_str().unwrap().contains("DELETE_CONTENT"));
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles/7/merge",
            Some(json!({ "target_id": "9" })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    let (s, _) = e
        .call(Some(OWNER), Method::DELETE, "/admin/cinema/titles/7", None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // Merging works for the owner; a title cannot merge into itself.
    let (s, body) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/cinema/titles/7/merge",
            Some(json!({ "target_id": "9" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "9");
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/cinema/titles/7/merge",
            Some(json!({ "target_id": "7" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Unpublishing is open to the curator (PUBLISH_MEDIA); an unknown title is 404.
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles/7/unpublish",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles/70/unpublish",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Collections are MANAGE_COLLECTIONS, which the curator holds.
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/collections",
            Some(json!({ "slug": "Picks", "name": "Picks", "surfaces": ["discover"], "published": true })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    assert_eq!(body["slug"], "picks");
    assert_eq!(body["id"], "4242");
}

#[tokio::test]
async fn creating_a_title_normalises_returns_string_ids_and_is_logged() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles",
            Some(title_body()),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    assert_eq!(body["id"], "4242");
    assert_eq!(body["title"], "New Film");
    assert_eq!(body["genres"], json!(["drama"]));
    assert_eq!(body["published"], false);
    assert!(
        body["poster_url"]
            .as_str()
            .unwrap()
            .starts_with("/cdn/cinema/4242/poster.webp?ex=")
    );
    let log = e.admin.log_rows();
    let row = log
        .iter()
        .find(|r| r.action == "cinema.title.create")
        .unwrap();
    assert_eq!(row.actor_id, CURATOR);
    assert_eq!(row.target_id, Some(4242));
    // Bad input is a client error, not a 500.
    for bad in [
        json!({ "kind": "movie", "title": "   " }),
        json!({ "kind": "movie", "title": "x", "year": 1500 }),
        json!({ "kind": "movie", "title": "x", "poster_key": "library/1.webp" }),
        json!({ "kind": "documentary", "title": "x" }),
    ] {
        let (s, _) = e
            .call(
                Some(CURATOR),
                Method::POST,
                "/admin/cinema/titles",
                Some(bad.clone()),
            )
            .await;
        assert!(s.is_client_error(), "{bad} -> {s}");
    }
    // The asset link answers with the staff-only field, and every change is logged.
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::PUT,
            "/admin/cinema/titles/7/asset",
            Some(json!({ "asset_id": "123" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["asset_id"], "123");
    assert!(
        e.admin
            .log_rows()
            .iter()
            .any(|r| r.action == "cinema.title.asset")
    );
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::PUT,
            "/admin/cinema/titles/7/asset",
            Some(json!({ "asset_id": "nope" })),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "an id that names nothing");
    // Markers: validated, ids generated.
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::PUT,
            "/admin/cinema/titles/7/markers",
            Some(json!({ "markers": [{ "kind": "credits", "start_ms": 5, "end_ms": 9 }] })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["markers"][0]["kind"], "credits");
    assert_eq!(body["markers"][0]["id"], "4242");
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::PUT,
            "/admin/cinema/titles/7/markers",
            Some(json!({ "markers": [{ "kind": "credits", "start_ms": 9, "end_ms": 5 }] })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A conflict from the repository is a 409.
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles/7/seasons",
            Some(json!({ "number": 1 })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn age_filter_defaults_to_showing_everything() {
    let e = env().await;
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], "7");
    assert!(
        body["items"][0].get("asset_id").is_none(),
        "members never see assets"
    );
    assert!(body["next_cursor"].is_null());
    // A member who hides restricted titles gets `hide`; an explicit `age` wins; staff lists are staff-scoped.
    e.call(Some(HIDER), Method::GET, "/cinema/titles", None)
        .await;
    e.call(Some(HIDER), Method::GET, "/cinema/titles?age=only", None)
        .await;
    e.call(Some(MEMBER), Method::GET, "/cinema/titles?age=hide", None)
        .await;
    e.call(Some(CURATOR), Method::GET, "/admin/cinema/titles", None)
        .await;
    let seen = e.browses();
    assert_eq!(seen.len(), 5, "one browse per call");
    assert_eq!(
        seen[0],
        Browsed {
            scope: Scope::Public,
            age: AgeFilter::Include,
            sort: SortKey::Updated,
            after: false,
            collection: None
        }
    );
    assert_eq!(seen[1].age, AgeFilter::Hide);
    assert_eq!(seen[2].age, AgeFilter::Only);
    assert_eq!(seen[3].age, AgeFilter::Hide);
    assert_eq!(seen[4].scope, Scope::Staff);
    assert_eq!(seen[4].age, AgeFilter::Include);
}

#[tokio::test]
async fn restricted_titles_are_visible_but_opening_needs_age_confirmation() {
    let e = env().await;
    for path in [
        "/cinema/titles/9",
        "/cinema/titles/9/episodes",
        "/cinema/titles/9/markers",
    ] {
        let (s, body) = e.call(Some(MEMBER), Method::GET, path, None).await;
        assert_eq!(s, StatusCode::OK, "{path}: {body}");
    }
    let (_, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles/9", None)
        .await;
    assert_eq!(body["title"]["age_restricted"], true);
    assert_eq!(body["title"]["playable"], true);
    // The plain title opens for everyone.
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles/7/play", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["asset_id"], "507");
    assert_eq!(body["title_id"], "7");
    // The restricted one answers 403 with the age code until the account confirmed.
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles/9/play", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], 900011);
    let (s, body) = e
        .call(Some(ADULT), Method::GET, "/cinema/titles/9/play", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["age_restricted"], true);
    // POST /sr/age-gate adds the account to the gate; the retried request succeeds.
    e.age.0.lock().unwrap().insert(UserId::new(MEMBER as u64));
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles/9/play", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    // Unknown things are 404.
    for path in [
        "/cinema/titles/70",
        "/cinema/titles/abc",
        "/cinema/episodes/5",
        "/cinema/episodes/5/play",
        "/cinema/titles/7/play?variant=1",
    ] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, path, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn bad_parameters_are_client_errors() {
    let e = env().await;
    for path in [
        "/cinema/titles?order=sideways",
        "/cinema/titles?year_from=2000&year_to=1990",
        "/cinema/titles?sort=curated",
        "/cinema/titles?sort=popularity",
        "/cinema/titles?kind=documentary",
        "/cinema/titles?age=maybe",
        "/cinema/titles?cursor=junk",
        "/cinema/titles?language=english-language",
        "/cinema/titles?runtime_min=200&runtime_max=100",
        "/cinema/titles/7/episodes?after=junk",
        "/cinema/collections?surface=elsewhere",
    ] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, path, None).await;
        assert!(s.is_client_error(), "{path} -> {s}");
    }
    // Good ones.
    for path in [
        "/cinema/titles?genres=drama,crime&exclude_genres=horror&rating=PG-13,R&sort=title&order=asc&limit=5",
        "/cinema/titles?collection=3&sort=curated",
        "/cinema/search?q=bla&kind=anime&limit=3",
        "/cinema/facets?kind=movie",
        "/cinema/collections?surface=anime",
    ] {
        let (s, body) = e.call(Some(MEMBER), Method::GET, path, None).await;
        assert_eq!(s, StatusCode::OK, "{path}: {body}");
    }
    let seen = e.browses();
    assert!(
        seen.iter()
            .any(|b| b.collection == Some(3) && b.sort == SortKey::Curated)
    );
}

#[tokio::test]
async fn rows_by_key_reach_generated_curated_and_provider_rows() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/cinema/rows/new-this-week?limit=3",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["key"], "new-this-week");
    assert_eq!(body["kind"], "generated");
    assert_eq!(
        body["items"][0]["id"], "7",
        "title fields sit at the top level of a row item"
    );
    assert!(body["items"][0].get("episode_id").is_none());
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/rows/genre:Drama", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["explore"], "genres=drama");
    // The provider hears the viewer and every parameter except limit.
    let (s, body) = e
        .call(
            Some(ADULT),
            Method::GET,
            "/cinema/rows/trending?scope=friends&limit=4",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["kind"], "provided");
    assert_eq!(body["title"], "Trending");
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        1,
        "unknown titles are dropped"
    );
    let asked = e.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].0, asked[0].1), (ADULT, 4));
    assert_eq!(asked[0].2.get("scope").map(String::as_str), Some("friends"));
    assert_eq!(asked[0].2.len(), 1);
    // Unknown and unprovided keys are 404; a collection that does not exist too.
    for key in [
        "nonsense",
        "continue-watching",
        "collection:5",
        "collection:x",
    ] {
        let (s, _) = e
            .call(
                Some(MEMBER),
                Method::GET,
                &format!("/cinema/rows/{key}"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{key}");
    }
    // Discover places the provider's row among the generated ones.
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/discover", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["hero"]["display"], "hero");
    assert_eq!(body["hero"]["key"], "tonights-feature");
    let keys: Vec<&str> = body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert!(
        keys.contains(&"trending")
            && keys.contains(&"new-this-week")
            && keys.contains(&"anime-room"),
        "{keys:?}"
    );
}

#[tokio::test]
async fn artwork_uploads_reserve_a_key_under_the_title() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles/7/artwork",
            Some(json!({ "slot": "backdrop", "content_type": "image/jpeg" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["key"], "cinema/7/backdrop-4242.jpg");
    assert_eq!(body["content_type"], "image/jpeg");
    assert!(!body["upload_url"].as_str().unwrap().is_empty());
    assert!(
        e.admin
            .log_rows()
            .iter()
            .any(|r| r.action == "cinema.artwork.reserve")
    );
    for (path, bad) in [
        (
            "/admin/cinema/titles/7/artwork",
            json!({ "slot": "poster", "content_type": "image/gif" }),
        ),
        (
            "/admin/cinema/titles/7/artwork",
            json!({ "slot": "banner", "content_type": "image/png" }),
        ),
    ] {
        let (s, _) = e
            .call(Some(CURATOR), Method::POST, path, Some(bad.clone()))
            .await;
        assert!(s.is_client_error(), "{bad}");
    }
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/cinema/titles/70/artwork",
            Some(json!({ "slot": "poster", "content_type": "image/png" })),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_missing_service_is_a_server_error_not_a_client_error() {
    let e = env_with(false).await;
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles", None)
        .await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn search_is_served_from_the_index() {
    let e = env().await;
    // The index is empty (it is rebuilt from the database at boot), so nothing matches yet.
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/cinema/search?q=stub", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["titles"], json!([]));
    assert_eq!(body["people"], json!([]));
}

#[test]
fn openapi_lists_every_cinema_path() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/cinema/discover",
        "/cinema/rows/{key}",
        "/cinema/collections",
        "/cinema/titles",
        "/cinema/facets",
        "/cinema/titles/{title_id}",
        "/cinema/titles/{title_id}/episodes",
        "/cinema/titles/{title_id}/markers",
        "/cinema/titles/{title_id}/play",
        "/cinema/episodes/{episode_id}",
        "/cinema/episodes/{episode_id}/markers",
        "/cinema/episodes/{episode_id}/play",
        "/cinema/search",
        "/cinema/assets/{asset_id}/playback",
        "/cinema/assets/{asset_id}/refresh",
        "/admin/cinema/titles",
        "/admin/cinema/titles/{title_id}/merge",
        "/admin/cinema/titles/{title_id}/asset",
        "/admin/cinema/seasons/{season_id}/episodes",
        "/admin/cinema/episodes/{episode_id}/publish",
        "/admin/cinema/episodes/{episode_id}/markers",
        "/admin/cinema/titles/{title_id}/variants",
        "/admin/cinema/titles/{title_id}/trailers",
        "/admin/cinema/collections/{collection_id}/items",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
    for schema in [
        "Title",
        "TitlePage",
        "Row",
        "RowItem",
        "PlayTarget",
        "Discover",
        "TitleDetail",
        "Facets",
    ] {
        assert!(doc["components"]["schemas"][schema].is_object(), "{schema}");
    }
    assert!(paths["/cinema/titles/{title_id}/play"]["get"]["responses"]["403"].is_object());
    // Operation ids are unique across the whole document.
    let full = serde_json::to_value(crate::routes::openapi()).unwrap();
    let mut ids = HashSet::new();
    for (_, item) in full["paths"].as_object().unwrap() {
        for (_, op) in item.as_object().unwrap() {
            if let Some(id) = op["operationId"].as_str().filter(|i| i.contains("cinema")) {
                assert!(ids.insert(id.to_owned()), "duplicate operationId {id}");
            }
        }
    }
}
