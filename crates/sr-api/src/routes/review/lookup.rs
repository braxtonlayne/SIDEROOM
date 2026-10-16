//! Metadata lookup for the submission editor (REV-ED-005, spec section 4): "Fetch details" asks a
//! catalog provider about a title and the member accepts the fields they want, one by one.
//!
//! * Off by default. A provider exists only when the operator enabled it in `[lookup]` (and gave
//!   its key where it needs one), and every request also needs the platform setting
//!   `review.lookup.enabled`. With nothing configured the routes answer `SrFeatureDisabled`.
//! * Requests leave from this server under [`sr_net::SafeClient`]; the browser never talks to a
//!   provider. Keys travel in headers or query strings of the server's own calls and are never
//!   logged or returned. Cover images are returned as paths of the signed external image proxy.
//! * Answers are cached for ten minutes per provider and query, so the editor cannot spend the
//!   operator's quota by retyping.
//! * Providers: AniList and MangaDex and MangaUpdates (no key), TMDB and ComicVine (operator key).

use super::MemberCtx;
use crate::error::ApiResult;
use crate::routes::Ext;
use crate::routes::settings::deps::invalid;
use crate::{HttpError, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{Json, Router, extract::Query, routing::get};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_core::ErrorCode;
use sr_net::{ApiCall, ApiMethod, SafeClient};
use sr_storage::cdn::CdnSigner;
use std::sync::Arc;
use std::time::Duration;
use utoipa::{IntoParams, ToSchema};

const CACHE_TTL: Duration = Duration::from_secs(600);
const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_QUERY_CHARS: usize = 200;
/// Most candidates one provider returns.
pub const MAX_CANDIDATES: usize = 8;

/// One catalog entry a provider found, as fields the editor can accept one by one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LookupCandidate {
    pub provider: String,
    /// The provider's id for the entry.
    pub external_id: String,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub year: Option<i32>,
    pub synopsis: String,
    pub authors: Vec<String>,
    pub genres: Vec<String>,
    /// The entry's page at the provider.
    pub url: Option<String>,
    /// Same-origin path of the proxied cover; empty when there is none.
    pub cover: String,
}

/// A provider's raw answer before the cover goes through the proxy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawCandidate {
    pub external_id: String,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub year: Option<i32>,
    pub synopsis: String,
    pub authors: Vec<String>,
    pub genres: Vec<String>,
    pub url: Option<String>,
    pub cover_url: Option<String>,
}

/// Why a provider could not answer. Never carries a URL or key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("lookup provider: {0}")]
pub struct LookupError(pub String);

/// The catalog sections a lookup serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Library,
    Cinema,
}

impl Section {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "library" => Some(Self::Library),
            "cinema" => Some(Self::Cinema),
            _ => None,
        }
    }
}

/// One metadata provider.
#[async_trait]
pub trait LookupProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn serves(&self, section: Section) -> bool;
    async fn search(
        &self,
        section: Section,
        query: &str,
        year: Option<i32>,
    ) -> Result<Vec<RawCandidate>, LookupError>;
}

/// The platform setting `review.lookup.enabled`, read for each request.
#[async_trait]
pub trait LookupSwitch: Send + Sync {
    async fn enabled(&self) -> bool;
}

/// Collaborators of the lookup routes; registered by the composition root. Without providers the
/// routes report the feature as off.
pub struct LookupDeps {
    providers: Vec<Arc<dyn LookupProvider>>,
    switch: Arc<dyn LookupSwitch>,
    signer: Option<CdnSigner>,
    cache: Cache<String, Arc<Vec<RawCandidate>>>,
}

impl LookupDeps {
    pub fn new(
        providers: Vec<Arc<dyn LookupProvider>>,
        switch: Arc<dyn LookupSwitch>,
        signer: Option<CdnSigner>,
    ) -> Self {
        Self {
            providers,
            switch,
            signer,
            cache: Cache::builder()
                .max_capacity(512)
                .time_to_live(CACHE_TTL)
                .build(),
        }
    }

    async fn active(&self, section: Section) -> Vec<&Arc<dyn LookupProvider>> {
        if self.providers.is_empty() || !self.switch.enabled().await {
            return Vec::new();
        }
        self.providers
            .iter()
            .filter(|p| p.serves(section))
            .collect()
    }

    fn render(&self, provider: &str, r: &RawCandidate) -> LookupCandidate {
        LookupCandidate {
            provider: provider.to_owned(),
            external_id: r.external_id.clone(),
            title: r.title.clone(),
            alt_titles: r.alt_titles.clone(),
            year: r.year,
            synopsis: r.synopsis.clone(),
            authors: r.authors.clone(),
            genres: r.genres.clone(),
            url: r.url.clone().filter(|u| https(u)),
            cover: r
                .cover_url
                .as_deref()
                .filter(|u| https(u))
                .and_then(|u| self.signer.as_ref().and_then(|s| s.external_path(u)))
                .unwrap_or_default(),
        }
    }
}

fn https(url: &str) -> bool {
    url.get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
}

// ---- providers -------------------------------------------------------------------

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Provider text as plain text: tags dropped, entities for the common cases decoded, whitespace
/// collapsed. Synopses from AniList and ComicVine arrive as HTML.
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ");
    clip(
        out.split_whitespace().collect::<Vec<_>>().join(" ").trim(),
        4000,
    )
}

fn year_of(date: &str) -> Option<i32> {
    date.get(..4)?
        .parse()
        .ok()
        .filter(|y| (1800..=2200).contains(y))
}

fn strings(v: &Value, max: usize) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str())
        .map(|s| clip(s, 120))
        .take(max)
        .collect()
}

async fn call_json(
    client: &SafeClient,
    url: url::Url,
    method: ApiMethod,
    headers: Vec<(String, String)>,
) -> Result<Value, LookupError> {
    let (status, body) = client
        .call(ApiCall {
            url,
            method,
            headers,
            basic: None,
            accept: "application/json",
            max_bytes: MAX_BYTES,
        })
        .await
        // The URL may carry a key: say what kind of failure, nothing else.
        .map_err(|e| LookupError(format!("request failed ({e})")))?;
    if !(200..300).contains(&status) {
        return Err(LookupError(format!("the provider answered {status}")));
    }
    serde_json::from_slice(&body).map_err(|_| LookupError("unreadable response".into()))
}

async fn post_json_value(
    client: &SafeClient,
    url: &url::Url,
    body: String,
) -> Result<Value, LookupError> {
    let r = client
        .post_json(
            url,
            vec![],
            body.into_bytes(),
            MAX_BYTES,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| LookupError(format!("request failed ({e})")))?;
    if !(200..300).contains(&r.status) {
        return Err(LookupError(format!("the provider answered {}", r.status)));
    }
    serde_json::from_slice(&r.body).map_err(|_| LookupError("unreadable response".into()))
}

fn new_client() -> SafeClient {
    SafeClient::production(sr_net::Policy::default(), "SideRoom-Lookup")
}

/// AniList GraphQL: manga for the Library, anime for Cinema.
pub struct AniList {
    client: SafeClient,
}

impl Default for AniList {
    fn default() -> Self {
        Self {
            client: new_client(),
        }
    }
}

const ANILIST_QUERY: &str = "query ($search: String, $type: MediaType) { Page(perPage: 8) { media(search: $search, type: $type, sort: SEARCH_MATCH) { id title { romaji english native } synonyms startDate { year } description(asHtml: false) genres coverImage { large } siteUrl staff(perPage: 4) { edges { role node { name { full } } } } } } }";

/// Read an AniList `Page.media` answer.
pub fn parse_anilist(v: &Value) -> Vec<RawCandidate> {
    v["data"]["Page"]["media"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let t = &m["title"];
            let title = t["english"]
                .as_str()
                .or_else(|| t["romaji"].as_str())
                .or_else(|| t["native"].as_str())?;
            let mut alts: Vec<String> = [&t["romaji"], &t["native"]]
                .into_iter()
                .filter_map(|x| x.as_str())
                .filter(|s| *s != title)
                .map(|s| clip(s, 200))
                .collect();
            alts.extend(strings(&m["synonyms"], 6));
            let authors = m["staff"]["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| {
                    e["role"]
                        .as_str()
                        .is_some_and(|r| r.contains("Story") || r.contains("Art"))
                })
                .filter_map(|e| e["node"]["name"]["full"].as_str())
                .map(|s| clip(s, 120))
                .collect();
            Some(RawCandidate {
                external_id: m["id"].as_i64()?.to_string(),
                title: clip(title, 300),
                alt_titles: alts,
                year: m["startDate"]["year"]
                    .as_i64()
                    .and_then(|y| i32::try_from(y).ok()),
                synopsis: plain_text(m["description"].as_str().unwrap_or("")),
                authors,
                genres: strings(&m["genres"], 12),
                url: m["siteUrl"].as_str().map(str::to_owned),
                cover_url: m["coverImage"]["large"].as_str().map(str::to_owned),
            })
        })
        .take(MAX_CANDIDATES)
        .collect()
}

#[async_trait]
impl LookupProvider for AniList {
    fn id(&self) -> &'static str {
        "anilist"
    }
    fn label(&self) -> &'static str {
        "AniList"
    }
    fn serves(&self, _: Section) -> bool {
        true
    }
    async fn search(
        &self,
        section: Section,
        query: &str,
        _year: Option<i32>,
    ) -> Result<Vec<RawCandidate>, LookupError> {
        let url = url::Url::parse("https://graphql.anilist.co")
            .map_err(|e| LookupError(e.to_string()))?;
        let kind = if section == Section::Cinema {
            "ANIME"
        } else {
            "MANGA"
        };
        let variables = json!({ "search": query, "type": kind });
        let body = json!({ "query": ANILIST_QUERY, "variables": variables }).to_string();
        let v = post_json_value(&self.client, &url, body).await?;
        Ok(parse_anilist(&v))
    }
}

/// MangaDex.
pub struct MangaDex {
    client: SafeClient,
}

impl Default for MangaDex {
    fn default() -> Self {
        Self {
            client: new_client(),
        }
    }
}

/// Read a MangaDex `/manga` answer (with `includes[]=cover_art&includes[]=author`).
pub fn parse_mangadex(v: &Value) -> Vec<RawCandidate> {
    let first = |o: &Value| -> Option<String> {
        o.as_object().and_then(|m| {
            m.get("en")
                .or_else(|| m.values().next())
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    };
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            let a = &m["attributes"];
            let title = first(&a["title"])?;
            let alts = a["altTitles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(first)
                .filter(|t| *t != title)
                .map(|t| clip(&t, 200))
                .take(8)
                .collect();
            let rel = |kind: &str| {
                m["relationships"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|r| r["type"] == kind)
                    .collect::<Vec<_>>()
            };
            let authors = rel("author")
                .into_iter()
                .chain(rel("artist"))
                .filter_map(|r| r["attributes"]["name"].as_str())
                .map(|s| clip(s, 120))
                .fold(Vec::new(), |mut acc, n| {
                    if !acc.contains(&n) {
                        acc.push(n);
                    }
                    acc
                });
            let cover = rel("cover_art")
                .first()
                .and_then(|r| r["attributes"]["fileName"].as_str())
                // File names are plain; refuse anything that could change the path.
                .filter(|f| !f.contains(['/', '\\', '?', '#']) && !f.contains(".."))
                .map(|f| format!("https://uploads.mangadex.org/covers/{id}/{f}.256.jpg"));
            let genres = a["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| t["attributes"]["group"] == "genre")
                .filter_map(|t| first(&t["attributes"]["name"]))
                .take(12)
                .collect();
            Some(RawCandidate {
                external_id: id.to_owned(),
                title: clip(&title, 300),
                alt_titles: alts,
                year: a["year"].as_i64().and_then(|y| i32::try_from(y).ok()),
                synopsis: first(&a["description"])
                    .map(|d| clip(&d, 4000))
                    .unwrap_or_default(),
                authors,
                genres,
                url: Some(format!("https://mangadex.org/title/{id}")),
                cover_url: cover,
            })
        })
        .take(MAX_CANDIDATES)
        .collect()
}

#[async_trait]
impl LookupProvider for MangaDex {
    fn id(&self) -> &'static str {
        "mangadex"
    }
    fn label(&self) -> &'static str {
        "MangaDex"
    }
    fn serves(&self, s: Section) -> bool {
        s == Section::Library
    }
    async fn search(
        &self,
        _: Section,
        query: &str,
        _year: Option<i32>,
    ) -> Result<Vec<RawCandidate>, LookupError> {
        let mut url = url::Url::parse("https://api.mangadex.org/manga")
            .map_err(|e| LookupError(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("title", query)
            .append_pair("limit", "8")
            .append_pair("includes[]", "cover_art")
            .append_pair("includes[]", "author")
            .append_pair("includes[]", "artist")
            .append_pair("order[relevance]", "desc");
        let v = call_json(&self.client, url, ApiMethod::Get, vec![]).await?;
        Ok(parse_mangadex(&v))
    }
}

/// MangaUpdates.
pub struct MangaUpdates {
    client: SafeClient,
}

impl Default for MangaUpdates {
    fn default() -> Self {
        Self {
            client: new_client(),
        }
    }
}

/// Read a MangaUpdates `series/search` answer.
pub fn parse_mangaupdates(v: &Value) -> Vec<RawCandidate> {
    v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let rec = &r["record"];
            let id = rec["series_id"].as_i64()?;
            Some(RawCandidate {
                external_id: id.to_string(),
                title: clip(&plain_text(rec["title"].as_str()?), 300),
                alt_titles: Vec::new(),
                year: rec["year"].as_str().and_then(|y| y.trim().parse().ok()),
                synopsis: plain_text(rec["description"].as_str().unwrap_or("")),
                authors: Vec::new(),
                genres: rec["genres"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|g| g["genre"].as_str())
                    .map(|s| clip(s, 120))
                    .take(12)
                    .collect(),
                url: rec["url"].as_str().map(str::to_owned),
                cover_url: rec["image"]["url"]["original"].as_str().map(str::to_owned),
            })
        })
        .take(MAX_CANDIDATES)
        .collect()
}

#[async_trait]
impl LookupProvider for MangaUpdates {
    fn id(&self) -> &'static str {
        "mangaupdates"
    }
    fn label(&self) -> &'static str {
        "MangaUpdates"
    }
    fn serves(&self, s: Section) -> bool {
        s == Section::Library
    }
    async fn search(
        &self,
        _: Section,
        query: &str,
        _year: Option<i32>,
    ) -> Result<Vec<RawCandidate>, LookupError> {
        let url = url::Url::parse("https://api.mangaupdates.com/v1/series/search")
            .map_err(|e| LookupError(e.to_string()))?;
        let body = json!({ "search": query, "perpage": 8 }).to_string();
        let (status, bytes) = self
            .client
            .post_json(
                &url,
                vec![],
                body.into_bytes(),
                MAX_BYTES,
                Duration::from_secs(10),
            )
            .await
            .map(|r| (r.status, r.body))
            .map_err(|e| LookupError(format!("request failed ({e})")))?;
        if !(200..300).contains(&status) {
            return Err(LookupError(format!("the provider answered {status}")));
        }
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| LookupError("unreadable response".into()))?;
        Ok(parse_mangaupdates(&v))
    }
}

/// TMDB (films and series), with the operator's key.
pub struct Tmdb {
    client: SafeClient,
    key: String,
}

impl Tmdb {
    pub fn new(key: &str) -> Self {
        Self {
            client: new_client(),
            key: key.to_owned(),
        }
    }
}

/// Read a TMDB `search/multi` answer. People are skipped.
pub fn parse_tmdb(v: &Value) -> Vec<RawCandidate> {
    v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| matches!(r["media_type"].as_str(), Some("movie" | "tv")))
        .filter_map(|r| {
            let tv = r["media_type"] == "tv";
            let title = r["title"].as_str().or_else(|| r["name"].as_str())?;
            let date = r["release_date"]
                .as_str()
                .or_else(|| r["first_air_date"].as_str());
            let id = r["id"].as_i64()?;
            let original = r["original_title"]
                .as_str()
                .or_else(|| r["original_name"].as_str());
            Some(RawCandidate {
                external_id: format!("{}:{id}", if tv { "tv" } else { "movie" }),
                title: clip(title, 300),
                alt_titles: original
                    .filter(|o| *o != title)
                    .map(|o| vec![clip(o, 300)])
                    .unwrap_or_default(),
                year: date.and_then(year_of),
                synopsis: clip(r["overview"].as_str().unwrap_or(""), 4000),
                authors: Vec::new(),
                genres: Vec::new(),
                url: Some(format!(
                    "https://www.themoviedb.org/{}/{id}",
                    if tv { "tv" } else { "movie" }
                )),
                cover_url: r["poster_path"]
                    .as_str()
                    .filter(|p| p.starts_with('/') && !p.contains(".."))
                    .map(|p| format!("https://image.tmdb.org/t/p/w185{p}")),
            })
        })
        .take(MAX_CANDIDATES)
        .collect()
}

#[async_trait]
impl LookupProvider for Tmdb {
    fn id(&self) -> &'static str {
        "tmdb"
    }
    fn label(&self) -> &'static str {
        "TMDB"
    }
    fn serves(&self, s: Section) -> bool {
        s == Section::Cinema
    }
    async fn search(
        &self,
        _: Section,
        query: &str,
        year: Option<i32>,
    ) -> Result<Vec<RawCandidate>, LookupError> {
        let mut url = url::Url::parse("https://api.themoviedb.org/3/search/multi")
            .map_err(|e| LookupError(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("query", query)
                .append_pair("include_adult", "false");
            if let Some(y) = year {
                q.append_pair("year", &y.to_string());
            }
            // A v3 key goes in the query; a v4 read token (a JWT) in the header.
            if !self.key.starts_with("eyJ") {
                q.append_pair("api_key", &self.key);
            }
        }
        let headers = if self.key.starts_with("eyJ") {
            vec![("Authorization".to_owned(), format!("Bearer {}", self.key))]
        } else {
            vec![]
        };
        let v = call_json(&self.client, url, ApiMethod::Get, headers).await?;
        Ok(parse_tmdb(&v))
    }
}

/// ComicVine, with the operator's key.
pub struct ComicVine {
    client: SafeClient,
    key: String,
}

impl ComicVine {
    pub fn new(key: &str) -> Self {
        Self {
            client: new_client(),
            key: key.to_owned(),
        }
    }
}

/// Read a ComicVine `search` answer (volumes).
pub fn parse_comicvine(v: &Value) -> Vec<RawCandidate> {
    v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(RawCandidate {
                external_id: r["id"].as_i64()?.to_string(),
                title: clip(r["name"].as_str()?, 300),
                alt_titles: Vec::new(),
                year: r["start_year"].as_str().and_then(|y| y.trim().parse().ok()),
                synopsis: plain_text(r["description"].as_str().unwrap_or("")),
                authors: r["publisher"]["name"]
                    .as_str()
                    .map(|p| vec![clip(p, 120)])
                    .unwrap_or_default(),
                genres: Vec::new(),
                url: r["site_detail_url"].as_str().map(str::to_owned),
                cover_url: r["image"]["small_url"].as_str().map(str::to_owned),
            })
        })
        .take(MAX_CANDIDATES)
        .collect()
}

#[async_trait]
impl LookupProvider for ComicVine {
    fn id(&self) -> &'static str {
        "comicvine"
    }
    fn label(&self) -> &'static str {
        "ComicVine"
    }
    fn serves(&self, s: Section) -> bool {
        s == Section::Library
    }
    async fn search(
        &self,
        _: Section,
        query: &str,
        _year: Option<i32>,
    ) -> Result<Vec<RawCandidate>, LookupError> {
        let mut url = url::Url::parse("https://comicvine.gamespot.com/api/search/")
            .map_err(|e| LookupError(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("api_key", &self.key)
            .append_pair("format", "json")
            .append_pair("resources", "volume")
            .append_pair("limit", "8")
            .append_pair(
                "field_list",
                "id,name,start_year,description,image,publisher,site_detail_url",
            )
            .append_pair("query", query);
        let v = call_json(&self.client, url, ApiMethod::Get, vec![]).await?;
        Ok(parse_comicvine(&v))
    }
}

// ---- routes ----------------------------------------------------------------------

/// A provider the editor can offer.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LookupProviderInfo {
    pub id: String,
    pub label: String,
}

/// What `GET /submissions/lookup/providers` returns.
#[derive(Debug, Serialize, ToSchema)]
pub struct LookupProviders {
    /// False while the feature is off (no provider configured, or the platform setting is off).
    pub enabled: bool,
    pub providers: Vec<LookupProviderInfo>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ProvidersQuery {
    /// `library` (default) or `cinema`.
    pub section: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct LookupQuery {
    /// Provider id from the providers list.
    pub provider: String,
    /// Title to look up, 2 to 200 characters.
    pub q: String,
    pub year: Option<i32>,
    /// `library` (default) or `cinema`.
    pub section: Option<String>,
}

/// Candidates found by one provider.
#[derive(Debug, Serialize, ToSchema)]
pub struct LookupResults {
    pub candidates: Vec<LookupCandidate>,
}

fn section_of(raw: &Option<String>) -> ApiResult<Section> {
    match raw.as_deref() {
        None | Some("") => Ok(Section::Library),
        Some(s) => Section::parse(s).ok_or_else(|| {
            invalid(
                "section",
                "LOOKUP_SECTION_INVALID",
                "Section must be library or cinema.",
            )
        }),
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/submissions/lookup/providers", get(providers))
        .route("/submissions/lookup", get(search))
}

/// The providers "Fetch details" can use for a section.
#[utoipa::path(
    get, path = "/submissions/lookup/providers", operation_id = "submissions_lookup_providers",
    params(ProvidersQuery),
    responses((status = 200, body = LookupProviders), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub(super) async fn providers(
    ctx: MemberCtx,
    Ext(deps): Ext<LookupDeps>,
    Query(q): Query<ProvidersQuery>,
) -> ApiResult<Json<LookupProviders>> {
    let _ = ctx;
    let section = section_of(&q.section)?;
    let active = deps.active(section).await;
    Ok(Json(LookupProviders {
        enabled: !active.is_empty(),
        providers: active
            .into_iter()
            .map(|p| LookupProviderInfo {
                id: p.id().to_owned(),
                label: p.label().to_owned(),
            })
            .collect(),
    }))
}

/// Ask one provider about a title (members; the provider must be on).
#[utoipa::path(
    get, path = "/submissions/lookup", operation_id = "submissions_lookup",
    params(LookupQuery),
    responses((status = 200, body = LookupResults), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Lookup is off or this provider is not configured."),
              (status = 502, body = ErrorBody)),
    security(("session" = []))
)]
pub(super) async fn search(
    ctx: MemberCtx,
    Ext(deps): Ext<LookupDeps>,
    Query(q): Query<LookupQuery>,
) -> ApiResult<Json<LookupResults>> {
    let _ = ctx;
    let section = section_of(&q.section)?;
    let text = q.q.trim();
    let n = text.chars().count();
    if !(2..=MAX_QUERY_CHARS).contains(&n) || text.chars().any(char::is_control) {
        return Err(invalid(
            "q",
            "LOOKUP_QUERY_INVALID",
            "Enter 2 to 200 characters to look up.",
        ));
    }
    let active = deps.active(section).await;
    let provider = active
        .into_iter()
        .find(|p| p.id() == q.provider)
        .ok_or_else(|| HttpError::from(ErrorCode::SrFeatureDisabled))?;
    let key = format!(
        "{}|{:?}|{}|{}",
        provider.id(),
        section == Section::Cinema,
        q.year.unwrap_or(0),
        text.to_lowercase()
    );
    let found = match deps.cache.get(&key).await {
        Some(hit) => hit,
        None => {
            let fresh = provider.search(section, text, q.year).await.map_err(|e| {
                tracing::warn!(provider = provider.id(), error = %e, "metadata lookup failed");
                HttpError::from(ErrorCode::SrInternal)
            })?;
            let v = Arc::new(fresh);
            deps.cache.insert(key, v.clone()).await;
            v
        }
    };
    Ok(Json(LookupResults {
        candidates: found
            .iter()
            .map(|r| deps.render(provider.id(), r))
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anilist_titles_synonyms_staff_and_html_are_read() {
        let v = json!({ "data": { "Page": { "media": [{
            "id": 30002,
            "title": { "romaji": "Berserk", "english": null, "native": "ベルセルク" },
            "synonyms": ["Berserk: The Prototype"],
            "startDate": { "year": 1989 },
            "description": "A <i>dark</i> tale.<br>Of a mercenary &amp; his sword.",
            "genres": ["Action", "Drama"],
            "coverImage": { "large": "https://s4.anilist.co/c.jpg" },
            "siteUrl": "https://anilist.co/manga/30002",
            "staff": { "edges": [
                { "role": "Story & Art", "node": { "name": { "full": "Kentaro Miura" } } },
                { "role": "Translator", "node": { "name": { "full": "Someone" } } }
            ] }
        }, { "id": 1, "title": { "romaji": null, "english": null, "native": null } }] } } });
        let got = parse_anilist(&v);
        assert_eq!(got.len(), 1, "an entry without a title is dropped");
        let c = &got[0];
        assert_eq!(c.title, "Berserk");
        assert_eq!(c.alt_titles, vec!["ベルセルク", "Berserk: The Prototype"]);
        assert_eq!(c.year, Some(1989));
        assert_eq!(c.synopsis, "A dark tale. Of a mercenary & his sword.");
        assert_eq!(c.authors, vec!["Kentaro Miura"]);
        assert_eq!(c.genres, vec!["Action", "Drama"]);
    }

    #[test]
    fn mangadex_fields_and_cover_url_are_built_safely() {
        let v = json!({ "data": [{
            "id": "801513ba-a712-498c-8f57-cae55b38cc92",
            "attributes": {
                "title": { "ja-ro": "Berserk" },
                "altTitles": [{ "en": "Berserk of Gluttony" }, { "ja": "ベルセルク" }],
                "description": { "en": "Guts." },
                "year": 1989,
                "tags": [
                    { "attributes": { "group": "genre", "name": { "en": "Action" } } },
                    { "attributes": { "group": "theme", "name": { "en": "Gore" } } }
                ]
            },
            "relationships": [
                { "type": "author", "attributes": { "name": "Miura Kentarou" } },
                { "type": "artist", "attributes": { "name": "Miura Kentarou" } },
                { "type": "cover_art", "attributes": { "fileName": "cover.jpg" } }
            ]
        }, {
            "id": "bad", "attributes": { "title": { "en": "X" } },
            "relationships": [{ "type": "cover_art", "attributes": { "fileName": "../../evil" } }]
        }] });
        let got = parse_mangadex(&v);
        assert_eq!(got[0].title, "Berserk");
        assert_eq!(got[0].year, Some(1989));
        assert_eq!(got[0].genres, vec!["Action"]);
        assert_eq!(got[0].authors, vec!["Miura Kentarou"], "listed once");
        assert_eq!(
            got[0].cover_url.as_deref(),
            Some(
                "https://uploads.mangadex.org/covers/801513ba-a712-498c-8f57-cae55b38cc92/cover.jpg.256.jpg"
            )
        );
        assert!(
            got[1].cover_url.is_none(),
            "a path-changing file name is refused"
        );
    }

    #[test]
    fn tmdb_skips_people_and_reads_movies_and_series() {
        let v = json!({ "results": [
            { "media_type": "person", "id": 5, "name": "Someone" },
            { "media_type": "movie", "id": 149, "title": "Akira", "original_title": "アキラ",
              "release_date": "1988-07-16", "overview": "Neo-Tokyo.", "poster_path": "/p.jpg" },
            { "media_type": "tv", "id": 30991, "name": "Cowboy Bebop", "first_air_date": "1998-10-24",
              "overview": "", "poster_path": "/../x" }
        ] });
        let got = parse_tmdb(&v);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].external_id, "movie:149");
        assert_eq!(got[0].year, Some(1988));
        assert_eq!(got[0].alt_titles, vec!["アキラ"]);
        assert_eq!(
            got[0].cover_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w185/p.jpg")
        );
        assert_eq!(got[1].external_id, "tv:30991");
        assert!(got[1].cover_url.is_none());
    }

    #[test]
    fn comicvine_and_mangaupdates_are_read() {
        let cv = json!({ "results": [{
            "id": 18166, "name": "Saga", "start_year": "2012",
            "description": "<p>An epic <b>space</b> opera.</p>",
            "image": { "small_url": "https://comicvine.gamespot.com/a/saga.jpg" },
            "publisher": { "name": "Image" },
            "site_detail_url": "https://comicvine.gamespot.com/saga/4050-18166/"
        }] });
        let got = parse_comicvine(&cv);
        assert_eq!(got[0].year, Some(2012));
        assert_eq!(got[0].synopsis, "An epic space opera.");
        assert_eq!(got[0].authors, vec!["Image"]);
        let mu = json!({ "results": [{ "record": {
            "series_id": 55, "title": "Blue &amp; Hour", "year": "2019",
            "description": "x", "genres": [{ "genre": "Drama" }],
            "url": "https://www.mangaupdates.com/series/55",
            "image": { "url": { "original": "https://cdn.mangaupdates.com/i.jpg" } }
        } }] });
        let got = parse_mangaupdates(&mu);
        assert_eq!(got[0].title, "Blue & Hour");
        assert_eq!(got[0].year, Some(2019));
        assert_eq!(got[0].genres, vec!["Drama"]);
    }

    #[test]
    fn plain_text_drops_tags_and_collapses_space() {
        assert_eq!(plain_text("<p>a</p>\n\n<p>b&nbsp;&lt;c&gt;</p>"), "a b <c>");
        assert_eq!(plain_text(""), "");
        assert_eq!(year_of("1988-07-16"), Some(1988));
        assert_eq!(year_of("x"), None);
    }

    struct Fixed(Vec<RawCandidate>);

    #[async_trait]
    impl LookupProvider for Fixed {
        fn id(&self) -> &'static str {
            "fixed"
        }
        fn label(&self) -> &'static str {
            "Fixed"
        }
        fn serves(&self, s: Section) -> bool {
            s == Section::Library
        }
        async fn search(
            &self,
            _: Section,
            _: &str,
            _: Option<i32>,
        ) -> Result<Vec<RawCandidate>, LookupError> {
            Ok(self.0.clone())
        }
    }

    struct Switch(bool);

    #[async_trait]
    impl LookupSwitch for Switch {
        async fn enabled(&self) -> bool {
            self.0
        }
    }

    #[tokio::test]
    async fn nothing_is_active_without_the_setting_or_without_providers() {
        let one: Vec<Arc<dyn LookupProvider>> = vec![Arc::new(Fixed(vec![]))];
        let off = LookupDeps::new(one, Arc::new(Switch(false)), None);
        assert!(off.active(Section::Library).await.is_empty());
        let none = LookupDeps::new(vec![], Arc::new(Switch(true)), None);
        assert!(none.active(Section::Library).await.is_empty());
        let one: Vec<Arc<dyn LookupProvider>> = vec![Arc::new(Fixed(vec![]))];
        let on = LookupDeps::new(one, Arc::new(Switch(true)), None);
        assert_eq!(on.active(Section::Library).await.len(), 1);
        assert!(
            on.active(Section::Cinema).await.is_empty(),
            "only for its sections"
        );
    }

    #[test]
    fn rendering_keeps_https_links_and_proxies_the_cover() {
        let deps = LookupDeps::new(
            vec![],
            Arc::new(Switch(true)),
            Some(CdnSigner::new(vec![9u8; 32])),
        );
        let raw = RawCandidate {
            title: "T".into(),
            url: Some("http://insecure.example/x".into()),
            cover_url: Some("https://img.example/c.jpg".into()),
            ..RawCandidate::default()
        };
        let c = deps.render("fixed", &raw);
        assert!(c.url.is_none(), "plain http links are dropped");
        assert!(c.cover.starts_with("/cdn/external/"), "{}", c.cover);
        let none = deps.render(
            "fixed",
            &RawCandidate {
                cover_url: Some("http://img.example/c.jpg".into()),
                ..raw
            },
        );
        assert_eq!(none.cover, "");
    }
}
