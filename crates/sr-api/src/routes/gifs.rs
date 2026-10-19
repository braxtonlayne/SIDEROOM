//! GIF picker backend (LNG-EXP-005): a privacy proxy in front of the operator's GIF
//! provider.
//!
//! * Off unless the operator configured a provider and API key (`[gif]`). `GET
//!   /gifs/config` says so; the other routes answer 403 `SrFeatureDisabled` then.
//! * The browser never talks to the provider: this server makes the search/trending
//!   calls (under the SSRF-safe client, key in the query string never logged) and the
//!   previews in the results are paths of the signed external image proxy
//!   (`/cdn/external/...`), which fetches, re-encodes and caches them.
//! * `url` of a result is the provider's original GIF URL; the client posts it as the
//!   message content and the unfurler turns it into an embed.
//! * Responses are cached for a minute per query, so a busy picker does not spend the
//!   operator's provider quota.

use crate::error::ApiResult;
use crate::routes::Ext;
use crate::routes::settings::deps::invalid;
use crate::{HttpError, Principal, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{Json, Router, extract::Query, routing::get};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_core::ErrorCode;
use sr_storage::cdn::CdnSigner;
use std::sync::Arc;
use std::time::Duration;
use utoipa::{IntoParams, OpenApi, ToSchema};

/// Most results per page.
pub const MAX_LIMIT: u32 = 50;
/// Default results per page.
pub const DEFAULT_LIMIT: u32 = 24;
const MAX_QUERY_CHARS: usize = 100;
const MAX_POS_CHARS: usize = 200;
const CACHE_TTL: Duration = Duration::from_secs(60);

/// One GIF as the provider reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGif {
    pub id: String,
    pub title: String,
    /// Original GIF (what gets posted).
    pub url: String,
    /// Small animated preview (proxied before it reaches a browser).
    pub preview_url: String,
    pub width: u32,
    pub height: u32,
}

/// One result page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawPage {
    pub results: Vec<RawGif>,
    /// Opaque continuation token; `None` on the last page.
    pub next: Option<String>,
}

/// A featured search term with a representative image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCategory {
    pub name: String,
    pub search_term: String,
    pub image_url: String,
}

/// Why the provider could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("gif provider: {0}")]
pub struct SourceError(pub String);

/// The provider seam; [`TenorSource`] is the production one.
#[async_trait]
pub trait GifSource: Send + Sync {
    async fn trending(&self, limit: u32, pos: Option<&str>) -> Result<RawPage, SourceError>;
    async fn search(&self, q: &str, limit: u32, pos: Option<&str>) -> Result<RawPage, SourceError>;
    async fn categories(&self) -> Result<Vec<RawCategory>, SourceError>;
}

// ---- Tenor v2 --------------------------------------------------------------------

/// The GIF region and locale an administrator can change while the server runs (D-300).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifRegion {
    pub country: String,
    pub locale: String,
}

/// Tenor API v2 over the SSRF-safe client.
pub struct TenorSource {
    client: sr_net::SafeClient,
    key: String,
    client_key: String,
    region: sr_net::Live<GifRegion>,
}

const TENOR_BASE: &str = "https://tenor.googleapis.com/v2";
const MAX_PROVIDER_BYTES: usize = 2 * 1024 * 1024;

impl TenorSource {
    pub fn new(key: &str, client_key: &str, country: &str, locale: &str) -> Self {
        Self {
            client: sr_net::SafeClient::production(sr_net::Policy::default(), "SideRoom-GifProxy"),
            key: key.to_owned(),
            client_key: client_key.to_owned(),
            region: sr_net::Live::new(GifRegion {
                country: country.to_owned(),
                locale: locale.to_owned(),
            }),
        }
    }

    /// Read the region and locale from a live handle on every request instead of the values
    /// given at construction.
    pub fn with_live_region(mut self, region: sr_net::Live<GifRegion>) -> Self {
        self.region = region;
        self
    }

    async fn fetch(&self, path: &str, extra: &[(&str, &str)]) -> Result<Value, SourceError> {
        let mut url = url::Url::parse(&format!("{TENOR_BASE}/{path}"))
            .map_err(|e| SourceError(e.to_string()))?;
        let region = self.region.load();
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("key", &self.key)
                .append_pair("client_key", &self.client_key)
                .append_pair("country", &region.country)
                .append_pair("locale", &region.locale)
                .append_pair("contentfilter", "medium");
            for (k, v) in extra {
                q.append_pair(k, v);
            }
        }
        let resp = self
            .client
            .get(&url, "application/json", MAX_PROVIDER_BYTES)
            .await
            // The URL carries the API key: report the kind of failure only.
            .map_err(|e| SourceError(format!("request failed ({e})")))?;
        serde_json::from_slice(&resp.body).map_err(|_| SourceError("unreadable response".into()))
    }

    async fn page(&self, path: &str, extra: &[(&str, &str)]) -> Result<RawPage, SourceError> {
        Ok(parse_tenor_page(&self.fetch(path, extra).await?))
    }
}

#[async_trait]
impl GifSource for TenorSource {
    async fn trending(&self, limit: u32, pos: Option<&str>) -> Result<RawPage, SourceError> {
        let limit = limit.to_string();
        let mut extra = vec![("limit", limit.as_str()), ("media_filter", "gif,tinygif")];
        if let Some(p) = pos {
            extra.push(("pos", p));
        }
        self.page("featured", &extra).await
    }

    async fn search(&self, q: &str, limit: u32, pos: Option<&str>) -> Result<RawPage, SourceError> {
        let limit = limit.to_string();
        let mut extra = vec![
            ("q", q),
            ("limit", limit.as_str()),
            ("media_filter", "gif,tinygif"),
        ];
        if let Some(p) = pos {
            extra.push(("pos", p));
        }
        self.page("search", &extra).await
    }

    async fn categories(&self) -> Result<Vec<RawCategory>, SourceError> {
        let v = self.fetch("categories", &[("type", "featured")]).await?;
        Ok(parse_tenor_categories(&v))
    }
}

fn https(url: &str) -> bool {
    url.get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
}

/// Read a Tenor `featured` / `search` response. Entries without a usable HTTPS GIF are
/// dropped.
pub fn parse_tenor_page(v: &Value) -> RawPage {
    let results = v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let formats = &r["media_formats"];
            let gif = &formats["gif"];
            let tiny = if formats["tinygif"]["url"].is_string() {
                &formats["tinygif"]
            } else {
                gif
            };
            let url = gif["url"].as_str()?;
            let preview = tiny["url"].as_str()?;
            if !https(url) || !https(preview) {
                return None;
            }
            let dim = |f: &Value, i: usize| {
                f["dims"][i]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(0)
            };
            Some(RawGif {
                id: r["id"].as_str()?.to_owned(),
                title: r["content_description"]
                    .as_str()
                    .or_else(|| r["title"].as_str())
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect(),
                url: url.to_owned(),
                preview_url: preview.to_owned(),
                width: dim(tiny, 0),
                height: dim(tiny, 1),
            })
        })
        .collect();
    RawPage {
        results,
        next: v["next"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
    }
}

/// Read a Tenor `categories` response.
pub fn parse_tenor_categories(v: &Value) -> Vec<RawCategory> {
    v["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let image = t["image"].as_str().filter(|u| https(u))?;
            let term = t["searchterm"].as_str()?;
            Some(RawCategory {
                name: t["name"]
                    .as_str()
                    .unwrap_or(term)
                    .trim_start_matches('#')
                    .to_owned(),
                search_term: term.to_owned(),
                image_url: image.to_owned(),
            })
        })
        .collect()
}

// ---- deps and routes -------------------------------------------------------------

/// Collaborators of the GIF endpoints; registered on the composition root. Without a
/// source the picker reports itself as off.
pub struct GifDeps {
    source: Option<Arc<dyn GifSource>>,
    signer: Option<CdnSigner>,
    cache: Cache<String, Arc<GifPageObject>>,
}

impl GifDeps {
    pub fn new(source: Option<Arc<dyn GifSource>>, signer: Option<CdnSigner>) -> Self {
        Self {
            source,
            signer,
            cache: Cache::builder()
                .max_capacity(512)
                .time_to_live(CACHE_TTL)
                .build(),
        }
    }

    fn proxied(&self, url: &str) -> String {
        self.signer
            .as_ref()
            .and_then(|s| s.external_path(url))
            .unwrap_or_default()
    }

    fn require(&self) -> ApiResult<&Arc<dyn GifSource>> {
        self.source
            .as_ref()
            .ok_or_else(|| HttpError::from(ErrorCode::SrFeatureDisabled))
    }
}

fn upstream(e: &SourceError) -> HttpError {
    tracing::warn!(error = %e, "gif provider failed");
    HttpError::from(ErrorCode::SrInternal)
}

/// Whether the GIF picker is available on this installation.
#[derive(Debug, Serialize, ToSchema)]
pub struct GifConfigObject {
    pub enabled: bool,
}

/// One GIF in the picker.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GifObject {
    pub id: String,
    pub title: String,
    /// The original GIF URL; post this as the message content.
    pub url: String,
    /// Same-origin path of the proxied preview.
    pub preview_url: String,
    pub width: u32,
    pub height: u32,
}

/// A page of results.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GifPageObject {
    pub results: Vec<GifObject>,
    /// Pass as `pos` for the next page; absent on the last page.
    pub next: Option<String>,
}

/// A featured search term.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GifCategoryObject {
    pub name: String,
    /// Search with this term when the tile is chosen.
    pub search_term: String,
    pub image_url: String,
}

/// The featured categories.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GifCategoriesObject {
    pub categories: Vec<GifCategoryObject>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PageQuery {
    /// Results per page, 1 to 50 (default 24).
    pub limit: Option<u32>,
    /// Continuation token from the previous page.
    pub pos: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    /// Search text, 1 to 100 characters.
    pub q: String,
    pub limit: Option<u32>,
    pub pos: Option<String>,
}

fn limit_of(raw: Option<u32>) -> u32 {
    raw.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn pos_of(raw: &Option<String>) -> ApiResult<Option<&str>> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(p)
            if p.chars().count() <= MAX_POS_CHARS && p.chars().all(|c| c.is_ascii_graphic()) =>
        {
            Ok(Some(p))
        }
        Some(_) => Err(invalid("pos", "GIF_POS_INVALID", "Invalid page token.")),
    }
}

fn render(deps: &GifDeps, page: RawPage) -> GifPageObject {
    GifPageObject {
        results: page
            .results
            .into_iter()
            .map(|g| GifObject {
                preview_url: deps.proxied(&g.preview_url),
                id: g.id,
                title: g.title,
                url: g.url,
                width: g.width,
                height: g.height,
            })
            .filter(|g| !g.preview_url.is_empty())
            .collect(),
        next: page.next,
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/gifs/config", get(config))
        .route("/gifs/trending", get(trending))
        .route("/gifs/search", get(search))
        .route("/gifs/categories", get(categories))
}

/// Whether the GIF picker has a provider.
#[utoipa::path(
    get,
    path = "/gifs/config",
    responses((status = 200, body = GifConfigObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn config(_p: Principal, Ext(deps): Ext<GifDeps>) -> Json<GifConfigObject> {
    Json(GifConfigObject {
        enabled: deps.source.is_some(),
    })
}

/// Trending GIFs.
#[utoipa::path(
    get,
    path = "/gifs/trending",
    params(PageQuery),
    responses((status = 200, body = GifPageObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn trending(
    _p: Principal,
    Ext(deps): Ext<GifDeps>,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<GifPageObject>> {
    let source = deps.require()?;
    let (limit, pos) = (limit_of(q.limit), pos_of(&q.pos)?);
    let key = format!("t|{limit}|{}", pos.unwrap_or(""));
    let page = match deps.cache.get(&key).await {
        Some(hit) => hit,
        None => {
            let fresh = source
                .trending(limit, pos)
                .await
                .map_err(|e| upstream(&e))?;
            let v = Arc::new(render(&deps, fresh));
            deps.cache.insert(key, v.clone()).await;
            v
        }
    };
    Ok(Json((*page).clone()))
}

/// Search GIFs.
#[utoipa::path(
    get,
    path = "/gifs/search",
    params(SearchQuery),
    responses((status = 200, body = GifPageObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn search(
    _p: Principal,
    Ext(deps): Ext<GifDeps>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<GifPageObject>> {
    let source = deps.require()?;
    let text = q.q.trim();
    if text.is_empty() || text.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid(
            "q",
            "GIF_QUERY_INVALID",
            "Search text must be 1 to 100 characters.",
        ));
    }
    let (limit, pos) = (limit_of(q.limit), pos_of(&q.pos)?);
    let key = format!("s|{limit}|{}|{}", pos.unwrap_or(""), text.to_lowercase());
    let page = match deps.cache.get(&key).await {
        Some(hit) => hit,
        None => {
            let fresh = source
                .search(text, limit, pos)
                .await
                .map_err(|e| upstream(&e))?;
            let v = Arc::new(render(&deps, fresh));
            deps.cache.insert(key, v.clone()).await;
            v
        }
    };
    Ok(Json((*page).clone()))
}

/// Featured categories for the picker's start page.
#[utoipa::path(
    get,
    path = "/gifs/categories",
    responses((status = 200, body = GifCategoriesObject), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn categories(
    _p: Principal,
    Ext(deps): Ext<GifDeps>,
) -> ApiResult<Json<GifCategoriesObject>> {
    let source = deps.require()?;
    let raw = source.categories().await.map_err(|e| upstream(&e))?;
    Ok(Json(GifCategoriesObject {
        categories: raw
            .into_iter()
            .map(|c| GifCategoryObject {
                image_url: deps.proxied(&c.image_url),
                name: c.name,
                search_term: c.search_term,
            })
            .filter(|c| !c.image_url.is_empty())
            .collect(),
    }))
}

#[derive(OpenApi)]
#[openapi(
    paths(config, trending, search, categories),
    components(schemas(
        ErrorBody,
        GifConfigObject,
        GifObject,
        GifPageObject,
        GifCategoryObject,
        GifCategoriesObject
    ))
)]
pub struct GifApi;

/// The OpenAPI fragment for the GIF endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    GifApi::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;
    use serde_json::json;
    use std::sync::Mutex;

    fn tenor_sample() -> Value {
        json!({
            "results": [
                {
                    "id": "1",
                    "content_description": "a dancing cat",
                    "media_formats": {
                        "gif": {"url": "https://media.tenor.com/a/cat.gif", "dims": [498, 280]},
                        "tinygif": {"url": "https://media.tenor.com/a/cat-tiny.gif", "dims": [220, 124]}
                    }
                },
                {
                    "id": "2",
                    "title": "only full size",
                    "media_formats": {"gif": {"url": "https://media.tenor.com/b/dog.gif", "dims": [300, 300]}}
                },
                {
                    "id": "3",
                    "media_formats": {"gif": {"url": "http://insecure.example/x.gif", "dims": [1, 1]}}
                },
                {"id": "4", "media_formats": {}}
            ],
            "next": "CAoQAB"
        })
    }

    #[test]
    fn parses_tenor_pages_and_drops_unusable_entries() {
        let page = parse_tenor_page(&tenor_sample());
        assert_eq!(page.next.as_deref(), Some("CAoQAB"));
        assert_eq!(page.results.len(), 2);
        let cat = &page.results[0];
        assert_eq!(cat.title, "a dancing cat");
        assert_eq!(cat.url, "https://media.tenor.com/a/cat.gif");
        assert_eq!(cat.preview_url, "https://media.tenor.com/a/cat-tiny.gif");
        assert_eq!((cat.width, cat.height), (220, 124));
        // No tinygif: the full GIF previews itself.
        assert_eq!(page.results[1].preview_url, page.results[1].url);
        assert_eq!(
            parse_tenor_page(&json!({"results": [], "next": ""})).next,
            None
        );
    }

    #[test]
    fn parses_categories() {
        let v = json!({"tags": [
            {"searchterm": "happy", "name": "#happy", "image": "https://media.tenor.com/h.gif"},
            {"searchterm": "bad", "image": "http://x/y.gif"}
        ]});
        let c = parse_tenor_categories(&v);
        assert_eq!(c.len(), 1);
        assert_eq!(
            (c[0].name.as_str(), c[0].search_term.as_str()),
            ("happy", "happy")
        );
    }

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl GifSource for Fake {
        async fn trending(&self, limit: u32, pos: Option<&str>) -> Result<RawPage, SourceError> {
            self.calls.lock().unwrap().push(format!("t{limit}{pos:?}"));
            Ok(parse_tenor_page(&tenor_sample()))
        }
        async fn search(
            &self,
            q: &str,
            limit: u32,
            _pos: Option<&str>,
        ) -> Result<RawPage, SourceError> {
            self.calls.lock().unwrap().push(format!("s{q}{limit}"));
            Ok(parse_tenor_page(&tenor_sample()))
        }
        async fn categories(&self) -> Result<Vec<RawCategory>, SourceError> {
            Ok(vec![RawCategory {
                name: "happy".into(),
                search_term: "happy".into(),
                image_url: "https://media.tenor.com/h.gif".into(),
            }])
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: 1,
            kind: AuthKind::Session { session_id: 1 },
        }
    }

    fn deps(source: Option<Arc<dyn GifSource>>) -> Arc<GifDeps> {
        Arc::new(GifDeps::new(source, Some(CdnSigner::new(vec![7; 32]))))
    }

    #[tokio::test]
    async fn off_without_a_provider() {
        let d = deps(None);
        assert!(!config(me(), Ext(d.clone())).await.0.enabled);
        let q = PageQuery {
            limit: None,
            pos: None,
        };
        assert!(trending(me(), Ext(d.clone()), Query(q)).await.is_err());
        assert!(categories(me(), Ext(d)).await.is_err());
    }

    #[tokio::test]
    async fn results_carry_proxied_previews_and_are_cached() {
        let fake = Arc::new(Fake::default());
        let d = deps(Some(fake.clone()));
        assert!(config(me(), Ext(d.clone())).await.0.enabled);
        let page = |limit| PageQuery { limit, pos: None };
        let Json(p) = trending(me(), Ext(d.clone()), Query(page(None)))
            .await
            .unwrap();
        assert_eq!(p.results.len(), 2);
        assert!(p.results[0].preview_url.starts_with("/cdn/external/"));
        assert_eq!(p.results[0].url, "https://media.tenor.com/a/cat.gif");
        assert!(!p.results[0].preview_url.contains("tenor"), "origin hidden");
        // Same query again: served from the cache.
        let _ = trending(me(), Ext(d.clone()), Query(page(None)))
            .await
            .unwrap();
        // limit is clamped to 50.
        let _ = trending(me(), Ext(d.clone()), Query(page(Some(500))))
            .await
            .unwrap();
        assert_eq!(
            fake.calls.lock().unwrap().clone(),
            vec!["t24None".to_owned(), "t50None".to_owned()]
        );
    }

    #[tokio::test]
    async fn search_validates_its_input() {
        let d = deps(Some(Arc::new(Fake::default())));
        let s = |q: &str, pos: Option<&str>| SearchQuery {
            q: q.into(),
            limit: None,
            pos: pos.map(str::to_owned),
        };
        assert!(
            search(me(), Ext(d.clone()), Query(s("  ", None)))
                .await
                .is_err()
        );
        assert!(
            search(me(), Ext(d.clone()), Query(s(&"x".repeat(101), None)))
                .await
                .is_err()
        );
        assert!(
            search(me(), Ext(d.clone()), Query(s("cat", Some("bad token"))))
                .await
                .is_err()
        );
        assert!(
            search(me(), Ext(d), Query(s("cat", Some("CAoQAB"))))
                .await
                .is_ok()
        );
    }
}
