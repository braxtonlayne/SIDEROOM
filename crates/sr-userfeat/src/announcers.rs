//! FTR-LIBF-001 library feed and FTR-WPA-001 watch party announcer: two modules that speak into a channel when
//! something happens elsewhere on the platform.
//!
//! The library feed polls for chapters and episodes published after a per-server cursor (set to "now" the first
//! time a server is seen, so switching it on does not replay history). Age-restricted series and titles are never
//! posted. The announcer is called by the watch party service when a party starts or ends.

use sqlx::FromRow;

use crate::UserFeatures;
use crate::config::Settings;
use crate::error::UfResult;
use crate::features::{LIBRARY_FEED_ID, WATCH_PARTY_ANNOUNCER_ID};
use crate::poster::{Card, Poster};

const BLUE: u32 = 0x3498DB;
const GREEN: u32 = 0x2ECC71;
/// Most cards one server gets per run (a bulk upload would otherwise flood the channel).
pub const MAX_CARDS_PER_RUN: usize = 5;
const FETCH_LIMIT: i64 = 25;

const CURSOR_CHAPTERS: &str = "library_feed.chapters";
const CURSOR_EPISODES: &str = "library_feed.episodes";

#[derive(Debug, Clone, FromRow)]
struct ChapterRow {
    id: i64,
    series_title: String,
    kind: String,
    genres: Vec<String>,
    number: String,
    title: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct EpisodeRow {
    id: i64,
    title_id: i64,
    series_title: String,
    kind: String,
    genres: Vec<String>,
    season: i32,
    number: i32,
    title: String,
}

/// What a feed accepts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedFilter {
    pub kinds: Vec<String>,
    pub genres: Vec<String>,
}

impl FeedFilter {
    pub fn from_settings(cfg: &Settings) -> Self {
        Self {
            kinds: cfg.strings("kinds"),
            genres: cfg.strings("genres"),
        }
    }

    /// Empty lists accept everything; genres match case-insensitively on any one.
    pub fn accepts(&self, kind: &str, genres: &[String]) -> bool {
        (self.kinds.is_empty() || self.kinds.iter().any(|k| k == kind))
            && (self.genres.is_empty()
                || genres
                    .iter()
                    .any(|g| self.genres.iter().any(|f| f.eq_ignore_ascii_case(g))))
    }
}

/// Trim a chapter number such as `12.00` to `12`.
pub fn chapter_label(number: &str) -> String {
    match number.split_once('.') {
        Some((whole, frac)) if frac.bytes().all(|b| b == b'0') => whole.to_owned(),
        _ => number.to_owned(),
    }
}

fn chapter_card(row: &ChapterRow, role: Option<i64>) -> Card {
    let label = chapter_label(&row.number);
    Card {
        content: role.map(|r| format!("<@&{r}>")).unwrap_or_default(),
        title: Some(format!("{} \u{2014} chapter {label}", row.series_title)),
        description: row.title.clone().filter(|t| !t.trim().is_empty()),
        url: Some(format!("/library/r/{}", row.id)),
        color: BLUE,
        footer: Some(format!("New in the Library \u{b7} {}", row.kind)),
        ping_roles: role.into_iter().collect(),
        ..Card::default()
    }
}

fn episode_card(row: &EpisodeRow, role: Option<i64>) -> Card {
    let code = format!("S{:02}E{:02}", row.season, row.number);
    Card {
        content: role.map(|r| format!("<@&{r}>")).unwrap_or_default(),
        title: Some(format!("{} \u{2014} {code}", row.series_title)),
        description: Some(row.title.clone()).filter(|t| !t.trim().is_empty()),
        url: Some(format!("/cinema/t/{}", row.title_id)),
        color: GREEN,
        footer: Some(format!("New in Cinema \u{b7} {}", row.kind)),
        ping_roles: role.into_iter().collect(),
        ..Card::default()
    }
}

impl UserFeatures {
    async fn has_cursor(&self, feature: &str, guild: i64) -> UfResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM feature_cursors WHERE feature = $1 AND guild_id = $2)",
        )
        .bind(feature)
        .bind(guild)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Start reading from now on.
    async fn init_cursor(&self, feature: &str, guild: i64) -> UfResult<()> {
        sqlx::query(
            "INSERT INTO feature_cursors (feature, guild_id, cursor_ts, cursor_id) \
             VALUES ($1, $2, now(), 0) ON CONFLICT DO NOTHING",
        )
        .bind(feature)
        .bind(guild)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Post what the library published since the server's cursor. Returns the number of cards posted.
    pub async fn run_library_feed(&self, poster: &dyn Poster, guild: i64) -> UfResult<usize> {
        if !self.enabled(LIBRARY_FEED_ID, Some(guild)).await? {
            return Ok(0);
        }
        let cfg = self.settings(LIBRARY_FEED_ID, Some(guild)).await?;
        let Some(channel) = cfg.id("channel") else {
            return Ok(0);
        };
        if !self.has_cursor(CURSOR_CHAPTERS, guild).await? {
            self.init_cursor(CURSOR_CHAPTERS, guild).await?;
            self.init_cursor(CURSOR_EPISODES, guild).await?;
            return Ok(0);
        }
        let filter = FeedFilter::from_settings(&cfg);
        let role = cfg.id("ping_role");
        let mut posted = 0;

        let chapters = sqlx::query_as::<_, ChapterRow>(
            "SELECT c.id, s.title AS series_title, s.kind, s.genres, \
                    c.number::text AS number, c.title \
             FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id \
             WHERE c.published AND c.published_at IS NOT NULL AND s.published AND s.merged_into IS NULL \
               AND NOT s.age_restricted \
               AND (c.published_at, c.id) > (SELECT cursor_ts, cursor_id FROM feature_cursors \
                                             WHERE feature = $1 AND guild_id = $2) \
             ORDER BY c.published_at, c.id LIMIT $3",
        )
        .bind(CURSOR_CHAPTERS)
        .bind(guild)
        .bind(FETCH_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        for row in &chapters {
            if filter.accepts(&row.kind, &row.genres) {
                if posted >= MAX_CARDS_PER_RUN {
                    // The rest follow on the next run.
                    return Ok(posted);
                }
                match poster.post(channel, chapter_card(row, role)).await {
                    Ok(_) => posted += 1,
                    Err(e) => {
                        tracing::warn!(guild, error = %e, "library feed could not post");
                        // Leave the cursor where it is so the card is retried on the next run.
                        return Ok(posted);
                    }
                }
            }
            sqlx::query(
                "UPDATE feature_cursors SET cursor_ts = (SELECT published_at FROM reading_chapters WHERE id = $3), \
                 cursor_id = $3 WHERE feature = $1 AND guild_id = $2",
            )
            .bind(CURSOR_CHAPTERS)
            .bind(guild)
            .bind(row.id)
            .execute(&self.pool)
            .await?;
        }

        let episodes = sqlx::query_as::<_, EpisodeRow>(
            "SELECT e.id, t.id AS title_id, t.title AS series_title, t.kind, t.genres, \
                    sn.number AS season, e.number, e.title \
             FROM video_episodes e JOIN video_titles t ON t.id = e.title_id \
                  JOIN video_seasons sn ON sn.id = e.season_id \
             WHERE e.published AND e.published_at IS NOT NULL AND t.published AND t.merged_into IS NULL \
               AND NOT t.age_restricted \
               AND (e.published_at, e.id) > (SELECT cursor_ts, cursor_id FROM feature_cursors \
                                             WHERE feature = $1 AND guild_id = $2) \
             ORDER BY e.published_at, e.id LIMIT $3",
        )
        .bind(CURSOR_EPISODES)
        .bind(guild)
        .bind(FETCH_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        for row in &episodes {
            if filter.accepts(&row.kind, &row.genres) {
                if posted >= MAX_CARDS_PER_RUN {
                    return Ok(posted);
                }
                match poster.post(channel, episode_card(row, role)).await {
                    Ok(_) => posted += 1,
                    Err(e) => {
                        tracing::warn!(guild, error = %e, "library feed could not post");
                        return Ok(posted);
                    }
                }
            }
            sqlx::query(
                "UPDATE feature_cursors SET cursor_ts = (SELECT published_at FROM video_episodes WHERE id = $3), \
                 cursor_id = $3 WHERE feature = $1 AND guild_id = $2",
            )
            .bind(CURSOR_EPISODES)
            .bind(guild)
            .bind(row.id)
            .execute(&self.pool)
            .await?;
        }
        Ok(posted)
    }

    /// Announce a watch party. `channel` is the channel the party is bound to; the server is looked up from it.
    /// Returns whether a message was posted.
    pub async fn announce_party(
        &self,
        poster: &dyn Poster,
        party: &PartyAnnouncement,
    ) -> UfResult<bool> {
        let guild: Option<i64> = sqlx::query_scalar("SELECT guild_id FROM channels WHERE id = $1")
            .bind(party.channel_id)
            .fetch_optional(&self.pool)
            .await?
            .flatten();
        let Some(guild) = guild else {
            return Ok(false);
        };
        if !self.enabled(WATCH_PARTY_ANNOUNCER_ID, Some(guild)).await? {
            return Ok(false);
        }
        let cfg = self.settings(WATCH_PARTY_ANNOUNCER_ID, Some(guild)).await?;
        if !party.started && !cfg.flag("announce_end", false) {
            return Ok(false);
        }
        let channel = cfg.id("channel").unwrap_or(party.channel_id);
        let role = cfg.id("ping_role").filter(|_| party.started);
        let card = party_card(party, role);
        match poster.post(channel, card).await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!(guild, error = %e, "watch party announcement failed");
                Ok(false)
            }
        }
    }
}

/// What the watch party service knows when it announces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyAnnouncement {
    pub party_id: i64,
    pub channel_id: i64,
    pub host_id: i64,
    pub title: String,
    pub episode_label: Option<String>,
    pub viewers: usize,
    pub started: bool,
}

pub fn party_card(p: &PartyAnnouncement, role: Option<i64>) -> Card {
    let what = match &p.episode_label {
        Some(l) if !l.is_empty() => format!("{} \u{2014} {l}", p.title),
        _ => p.title.clone(),
    };
    let mut content = String::new();
    if let Some(r) = role {
        content.push_str(&format!("<@&{r}> "));
    }
    if p.started {
        content.push_str(&format!("<@{}> started a watch party.", p.host_id));
    }
    Card {
        content,
        title: Some(what),
        description: Some(if p.started {
            "Join in from the Cinema.".to_owned()
        } else {
            format!("The watch party ended with {} watching.", p.viewers)
        }),
        url: p.started.then(|| format!("/cinema/party/{}", p.party_id)),
        color: if p.started { GREEN } else { BLUE },
        footer: Some("Watch party".into()),
        ping_roles: role.into_iter().collect(),
        ping_users: vec![p.host_id],
        ..Card::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filter_accepts_by_kind_and_genre() {
        let g = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let all = FeedFilter::default();
        assert!(all.accepts("manga", &g(&["Action"])));
        let f = FeedFilter {
            kinds: g(&["manga", "anime"]),
            genres: g(&["action"]),
        };
        assert!(f.accepts("manga", &g(&["Action", "Drama"])));
        assert!(!f.accepts("comic", &g(&["Action"])));
        assert!(!f.accepts("manga", &g(&["Drama"])));
        assert!(!f.accepts("manga", &[]));
        let kinds_only = FeedFilter {
            kinds: g(&["movie"]),
            genres: vec![],
        };
        assert!(kinds_only.accepts("movie", &[]));
    }

    #[test]
    fn filter_reads_settings() {
        let cfg = Settings(
            json!({"kinds": ["manga"], "genres": ["Action"]})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let f = FeedFilter::from_settings(&cfg);
        assert_eq!(f.kinds, vec!["manga"]);
        assert_eq!(f.genres, vec!["Action"]);
    }

    #[test]
    fn chapter_numbers_lose_trailing_zeros() {
        assert_eq!(chapter_label("12.00"), "12");
        assert_eq!(chapter_label("12.50"), "12.50");
        assert_eq!(chapter_label("7"), "7");
    }

    #[test]
    fn party_cards_ping_only_on_start() {
        let p = PartyAnnouncement {
            party_id: 9,
            channel_id: 2,
            host_id: 5,
            title: "Akira".into(),
            episode_label: None,
            viewers: 3,
            started: true,
        };
        let c = party_card(&p, Some(77));
        assert!(c.content.starts_with("<@&77> <@5>"));
        assert_eq!(c.url.as_deref(), Some("/cinema/party/9"));
        assert_eq!(c.ping_roles, vec![77]);
        let ended = party_card(
            &PartyAnnouncement {
                started: false,
                ..p
            },
            None,
        );
        assert!(ended.url.is_none());
        assert!(ended.description.unwrap_or_default().contains('3'));
    }
}
