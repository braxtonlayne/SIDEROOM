//! Rows of the Discover page and the browse pages (CIN-DATA-005).
//!
//! Three kinds of row:
//!
//! * **curated**: a staff collection (`collection:<id>`), including the hero spotlight;
//! * **generated**: defined here from the catalog alone (`new-this-week`, `recently-added`,
//!   `anime-room`, `one-more-episode`, `films`, `animation-banner`, `genre:<name>`);
//! * **provided**: rows that need user data (continue watching, trending scopes, because you
//!   watched, friends are watching, recently explored, requests fulfilled). The social backend
//!   registers a [`RowProvider`] per key with `CinemaService::with_provider`; the catalog only
//!   decides where the row sits on the page.
//!
//! A provider names titles (and optionally an episode, progress and a reason); the service loads
//! them through the viewer's scope, so a provider can never show a draft or a merged-away title.

use std::collections::BTreeMap;

use async_trait::async_trait;

use crate::error::CinemaResult;
use crate::model::*;
use crate::service::Viewer;

/// Provider-backed row keys.
pub const KEY_CONTINUE_WATCHING: &str = "continue-watching";
pub const KEY_RECENTLY_EXPLORED: &str = "recently-explored";
pub const KEY_TRENDING: &str = "trending";
pub const KEY_BECAUSE_YOU_WATCHED: &str = "because-you-watched";
pub const KEY_FRIENDS_WATCHING: &str = "friends-watching";
pub const KEY_REQUESTS_FULFILLED: &str = "requests-fulfilled";

/// Generated row keys.
pub const KEY_NEW_THIS_WEEK: &str = "new-this-week";
pub const KEY_RECENTLY_ADDED: &str = "recently-added";
pub const KEY_ANIME_ROOM: &str = "anime-room";
pub const KEY_ONE_MORE_EPISODE: &str = "one-more-episode";
pub const KEY_FILMS: &str = "films";
pub const KEY_ANIMATION_BANNER: &str = "animation-banner";
pub const KEY_HERO: &str = "tonights-feature";

/// What a provider is asked for.
#[derive(Debug, Clone)]
pub struct RowContext {
    pub viewer: Viewer,
    /// Titles wanted (1 to 50).
    pub limit: u32,
    /// The request's query parameters other than `limit`, such as `scope=friends`.
    pub params: BTreeMap<String, String>,
}

/// One title of a provided row.
#[derive(Debug, Clone, PartialEq)]
pub struct RowEntry {
    pub title_id: i64,
    pub episode_id: Option<i64>,
    pub progress_percent: Option<f32>,
    pub reason: Option<String>,
}

impl RowEntry {
    pub fn title(title_id: i64) -> Self {
        Self {
            title_id,
            episode_id: None,
            progress_percent: None,
            reason: None,
        }
    }
}

/// A row supplied by a provider. Unset text falls back to the catalog's default copy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProvidedRow {
    pub title: Option<String>,
    pub eyebrow: Option<String>,
    pub subtitle: Option<String>,
    pub explore: Option<String>,
    pub entries: Vec<RowEntry>,
}

/// A source of one user-data row.
#[async_trait]
pub trait RowProvider: Send + Sync {
    /// The row key this provider answers, for example [`KEY_CONTINUE_WATCHING`].
    fn key(&self) -> &'static str;

    /// The row for this viewer, or `None` when there is nothing to show (the row is left out).
    async fn row(&self, ctx: &RowContext) -> CinemaResult<Option<ProvidedRow>>;
}

/// Hears about catalog changes that other tables must follow. All methods default to nothing.
#[async_trait]
pub trait CatalogObserver: Send + Sync {
    /// `source` was merged into `target`. `episode_map` lists `(dropped duplicate episode,
    /// episode that replaced it)`; every other episode of `source` now belongs to `target`
    /// under the same id. Tables keyed by title or episode id should be re-pointed.
    async fn title_merged(&self, _source: i64, _target: i64, _episode_map: &EpisodeMap) {}

    /// A title and all its episodes were deleted.
    async fn title_deleted(&self, _title: i64, _episodes: &[i64]) {}

    /// Episodes were deleted (an episode, or a season with its episodes).
    async fn episodes_deleted(&self, _title: i64, _episodes: &[i64]) {}
}

/// The default copy of a provider row: `(title, eyebrow)`.
pub fn provided_defaults(key: &str) -> Option<(&'static str, Option<&'static str>)> {
    Some(match key {
        KEY_CONTINUE_WATCHING => ("Continue watching", None),
        KEY_RECENTLY_EXPLORED => ("Recently explored", None),
        KEY_TRENDING => ("Trending", None),
        KEY_BECAUSE_YOU_WATCHED => ("Because you watched", None),
        KEY_FRIENDS_WATCHING => ("Friends are watching", None),
        KEY_REQUESTS_FULFILLED => ("Requests fulfilled", None),
        _ => return None,
    })
}

/// A generated row: its copy and the browse query that fills it.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedRow {
    pub key: String,
    pub title: String,
    pub eyebrow: Option<String>,
    pub subtitle: Option<String>,
    pub display: CollectionDisplay,
    /// Query string for `GET /cinema/titles` listing the same set.
    pub explore: String,
    pub query: BrowseQuery,
}

const WEEK_MS: i64 = 7 * 24 * 3600 * 1000;

/// Percent-encode a query value.
pub fn encode_query(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The generated row for `key`, or `None` for a key that is not generated. `now_ms` anchors
/// "this week".
pub fn generated(key: &str, now_ms: i64, age: AgeFilter) -> Option<GeneratedRow> {
    let base = BrowseQuery {
        age,
        ..BrowseQuery::default()
    };
    let row = |title: &str, explore: &str, query: BrowseQuery| {
        Some(GeneratedRow {
            key: key.to_owned(),
            title: title.to_owned(),
            eyebrow: None,
            subtitle: None,
            display: CollectionDisplay::Row,
            explore: explore.to_owned(),
            query,
        })
    };
    if let Some(genre) = key.strip_prefix("genre:") {
        let genre = genre.trim().to_lowercase();
        if genre.is_empty() || genre.chars().count() > 60 {
            return None;
        }
        let title = {
            let mut c = genre.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        };
        return Some(GeneratedRow {
            key: format!("genre:{genre}"),
            title,
            eyebrow: None,
            subtitle: None,
            display: CollectionDisplay::Row,
            explore: format!("genres={}", encode_query(&genre)),
            query: BrowseQuery {
                genres: vec![genre],
                ..base
            },
        });
    }
    match key {
        KEY_NEW_THIS_WEEK => row(
            "New this week",
            "sort=updated",
            BrowseQuery {
                since_ms: Some(now_ms - WEEK_MS),
                ..base
            },
        ),
        KEY_RECENTLY_ADDED => row(
            "Recently added",
            "sort=newest",
            BrowseQuery {
                sort: SortKey::Newest,
                ..base
            },
        ),
        KEY_ANIME_ROOM => row(
            "The anime room",
            "kind=anime",
            BrowseQuery {
                kind: Some(TitleKind::Anime),
                ..base
            },
        ),
        KEY_ONE_MORE_EPISODE => row(
            "One more episode",
            "kind=series",
            BrowseQuery {
                episodic_only: true,
                ..base
            },
        ),
        KEY_FILMS => row(
            "For the love of cinema",
            "kind=movie",
            BrowseQuery {
                kind: Some(TitleKind::Movie),
                sort: SortKey::Newest,
                ..base
            },
        ),
        KEY_ANIMATION_BANNER => Some(GeneratedRow {
            key: key.to_owned(),
            title: "Animation. Without limits.".to_owned(),
            eyebrow: Some("A ROOM OF ITS OWN".to_owned()),
            subtitle: Some("Step into the anime room".to_owned()),
            display: CollectionDisplay::Banner,
            explore: "kind=anime".to_owned(),
            query: BrowseQuery {
                kind: Some(TitleKind::Anime),
                sort: SortKey::Newest,
                ..base
            },
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_rows_cover_the_documented_keys() {
        let now = 1_000_000_000_000;
        for key in [
            KEY_NEW_THIS_WEEK,
            KEY_RECENTLY_ADDED,
            KEY_ANIME_ROOM,
            KEY_ONE_MORE_EPISODE,
            KEY_FILMS,
            KEY_ANIMATION_BANNER,
            "genre:Science fiction",
        ] {
            let g = generated(key, now, AgeFilter::Include).unwrap_or_else(|| panic!("{key}"));
            assert!(!g.title.is_empty());
        }
        assert!(generated("nope", now, AgeFilter::Include).is_none());
        assert!(generated("genre:", now, AgeFilter::Include).is_none());
        assert!(generated(KEY_CONTINUE_WATCHING, now, AgeFilter::Include).is_none());
    }

    #[test]
    fn new_this_week_looks_back_seven_days() {
        let now = 10 * WEEK_MS;
        let g = generated(KEY_NEW_THIS_WEEK, now, AgeFilter::Hide).unwrap();
        assert_eq!(g.query.since_ms, Some(9 * WEEK_MS));
        assert_eq!(g.query.age, AgeFilter::Hide);
    }

    #[test]
    fn genre_rows_are_lower_cased_and_explorable() {
        let g = generated("genre:Science fiction", 0, AgeFilter::Include).unwrap();
        assert_eq!(g.key, "genre:science fiction");
        assert_eq!(g.title, "Science fiction");
        assert_eq!(g.explore, "genres=science%20fiction");
        assert_eq!(g.query.genres, vec!["science fiction"]);
    }

    #[test]
    fn banner_is_a_banner() {
        let g = generated(KEY_ANIMATION_BANNER, 0, AgeFilter::Include).unwrap();
        assert_eq!(g.display, CollectionDisplay::Banner);
        assert_eq!(g.query.kind, Some(TitleKind::Anime));
    }

    #[test]
    fn provided_copy() {
        assert!(provided_defaults(KEY_TRENDING).is_some());
        assert!(provided_defaults("new-this-week").is_none());
    }
}
